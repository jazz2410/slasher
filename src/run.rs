//! The run: playing a level, dying, and starting it again.
//!
//! A level's *data* — its collision grid, spawn points and art — is loaded once
//! and never mutated, so restarting is not a reload. It is a despawn of
//! everything the run owns, followed by respawning it from that data. That is
//! what makes a retry instant, which is the whole loop this game is built
//! around: die often, start again immediately, lose nothing but time.
//!
//! Every entity belonging to a run carries `DespawnOnExit(Run::Playing)`, so
//! leaving that state clears the board with no bookkeeping. Anything that should
//! survive a death — the camera, the loaded level, the character blueprints —
//! simply does not carry it.

use bevy::prelude::*;

use crate::character::Dying;
use crate::combat::{CombatSet, Health};
use crate::player::Player;

/// Pause between dying and the level coming back. Long enough to register what
/// happened, short enough that it never becomes a loading screen.
const RESTART_DELAY: f32 = 0.8;

pub struct RunPlugin;

impl Plugin for RunPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<Run>()
            // `bevy_state` inserts the first `StateTransition` *before*
            // `PreStartup`, so a default of `Playing` would fire `OnEnter`
            // before the level and the character blueprints exist. Starting in
            // `Loading` and stepping out of it once startup has run avoids that
            // entirely — and leaves the obvious seam for real asset loading.
            .add_systems(PostStartup, begin)
            .add_systems(
                Update,
                // Damage and the death clock both settle before this check. A
                // lethal hit starts the animation immediately; only its final
                // frame is allowed to advance the run to `Ended`.
                watch_for_death
                    .run_if(in_state(Run::Playing))
                    .after(CombatSet::Death),
            )
            .add_systems(OnEnter(Run::Ended), start_the_clock)
            .add_systems(Update, tick_restart.run_if(in_state(Run::Ended)));
    }
}

/// Where a run is.
///
/// Deliberately only two states. Everything a level needs to *begin* hangs off
/// entering `Playing`, so adding level transitions later means changing which
/// level is loaded, not adding more states.
#[derive(States, Default, Debug, Clone, PartialEq, Eq, Hash)]
pub enum Run {
    /// Nothing has spawned yet; the level and blueprints are still being read.
    #[default]
    Loading,
    Playing,
    /// The player is down. The board is cleared and a short clock runs.
    Ended,
}

/// Marks everything a single run owns. Restarting despawns all of it.
pub type RunScoped = DespawnOnExit<Run>;

/// The component to attach to anything spawned for a run.
pub fn run_scoped() -> RunScoped {
    DespawnOnExit(Run::Playing)
}

#[derive(Resource)]
struct RestartClock(Timer);

fn begin(mut next: ResMut<NextState<Run>>) {
    next.set(Run::Playing);
}

fn watch_for_death(
    player: Query<(&Health, Option<&Dying>), With<Player>>,
    mut next: ResMut<NextState<Run>>,
) {
    let Ok((health, dying)) = player.single() else {
        return;
    };
    if health.is_spent() && dying.is_some_and(Dying::is_done) {
        next.set(Run::Ended);
    }
}

fn start_the_clock(mut commands: Commands) {
    commands.insert_resource(RestartClock(Timer::from_seconds(
        RESTART_DELAY,
        TimerMode::Once,
    )));
}

fn tick_restart(
    time: Res<Time>,
    clock: Option<ResMut<RestartClock>>,
    mut next: ResMut<NextState<Run>>,
) {
    let Some(mut clock) = clock else { return };
    if clock.0.tick(time.delta()).is_finished() {
        next.set(Run::Playing);
    }
}
