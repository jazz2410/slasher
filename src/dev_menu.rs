//! Small in-game tools intended for level development, not the final UI.

use bevy::prelude::*;

use crate::level::{Level, LevelCatalog};
use crate::run::Run;

const PANEL: Color = Color::srgba(0.055, 0.045, 0.040, 0.97);
const BUTTON: Color = Color::srgb(0.18, 0.15, 0.13);
const BUTTON_HOVERED: Color = Color::srgb(0.30, 0.22, 0.16);
const BUTTON_SELECTED: Color = Color::srgb(0.48, 0.25, 0.10);
const GOLD: Color = Color::srgb(0.92, 0.62, 0.22);

pub struct DeveloperMenuPlugin;

impl Plugin for DeveloperMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DeveloperMenuState>()
            .add_systems(Startup, spawn_level_menu)
            .add_systems(Update, (toggle_level_menu, handle_level_buttons));
    }
}

#[derive(Resource, Default)]
struct DeveloperMenuState {
    open: bool,
}

#[derive(Component)]
struct LevelMenuRoot;

#[derive(Component)]
struct LevelButton {
    index: usize,
}

fn spawn_level_menu(mut commands: Commands, catalog: Option<Res<LevelCatalog>>) {
    let levels: Vec<(usize, String)> = catalog
        .as_deref()
        .map(|catalog| {
            catalog
                .levels()
                .map(|(index, name)| (index, name.to_owned()))
                .collect()
        })
        .unwrap_or_default();
    let selected = catalog.as_deref().map_or(0, LevelCatalog::selected);

    commands
        .spawn((
            Name::new("Developer Level Menu"),
            LevelMenuRoot,
            Visibility::Hidden,
            GlobalZIndex(1000),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.01, 0.008, 0.006, 0.74)),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Px(380.0),
                    min_height: Val::Px(210.0),
                    padding: UiRect::all(Val::Px(22.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    border_radius: BorderRadius::all(Val::Px(9.0)),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(12.0),
                    ..default()
                },
                BackgroundColor(PANEL),
                BorderColor::all(GOLD),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new("DEVELOPER LEVEL SELECT"),
                    TextFont::from_font_size(24.0),
                    TextColor(GOLD),
                ));
                panel.spawn((
                    Text::new("F1: close menu  |  click a level to reload"),
                    TextFont::from_font_size(14.0),
                    TextColor(Color::srgb(0.72, 0.68, 0.62)),
                ));

                if levels.is_empty() {
                    panel.spawn((
                        Text::new("No LDtk levels were loaded"),
                        TextFont::from_font_size(18.0),
                        TextColor(Color::srgb(0.9, 0.35, 0.25)),
                    ));
                }

                for (index, name) in levels {
                    panel
                        .spawn((
                            Name::new(format!("Select level {name}")),
                            Button,
                            LevelButton { index },
                            Node {
                                width: Val::Percent(100.0),
                                height: Val::Px(44.0),
                                padding: UiRect::horizontal(Val::Px(14.0)),
                                border: UiRect::all(Val::Px(1.0)),
                                border_radius: BorderRadius::all(Val::Px(5.0)),
                                align_items: AlignItems::Center,
                                ..default()
                            },
                            BackgroundColor(if index == selected {
                                BUTTON_SELECTED
                            } else {
                                BUTTON
                            }),
                            BorderColor::all(Color::srgb(0.42, 0.33, 0.25)),
                        ))
                        .with_child((
                            Text::new(name),
                            TextFont::from_font_size(19.0),
                            TextColor(Color::srgb(0.94, 0.90, 0.82)),
                        ));
                }
            });
        });
}

fn set_menu_open(
    open: bool,
    state: &mut DeveloperMenuState,
    time: &mut Time<Virtual>,
    roots: &mut Query<&mut Visibility, With<LevelMenuRoot>>,
) {
    state.open = open;
    if open {
        time.pause();
    } else {
        time.unpause();
    }
    for mut visibility in roots {
        *visibility = if open {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

fn toggle_level_menu(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<DeveloperMenuState>,
    mut time: ResMut<Time<Virtual>>,
    mut roots: Query<&mut Visibility, With<LevelMenuRoot>>,
) {
    let toggle = keys.just_pressed(KeyCode::F1);
    let close = state.open && keys.just_pressed(KeyCode::Escape);
    if toggle || close {
        let open = if close { false } else { !state.open };
        set_menu_open(open, &mut state, &mut time, &mut roots);
    }
}

fn handle_level_buttons(
    mut commands: Commands,
    mut buttons: Query<
        (&Interaction, &LevelButton, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>),
    >,
    mut catalog: ResMut<LevelCatalog>,
    mut next_run: ResMut<NextState<Run>>,
    mut state: ResMut<DeveloperMenuState>,
    mut time: ResMut<Time<Virtual>>,
    mut roots: Query<&mut Visibility, With<LevelMenuRoot>>,
) {
    for (interaction, button, mut background) in &mut buttons {
        match *interaction {
            Interaction::Pressed => {
                let Some(level) = catalog.select(button.index) else {
                    continue;
                };
                info!("developer selected level '{}'", level.name);
                commands.insert_resource::<Level>(level);
                *background = BackgroundColor(BUTTON_SELECTED);
                next_run.set(Run::Reloading);
                set_menu_open(false, &mut state, &mut time, &mut roots);
            }
            Interaction::Hovered => {
                *background = BackgroundColor(BUTTON_HOVERED);
            }
            Interaction::None => {
                *background = BackgroundColor(if button.index == catalog.selected() {
                    BUTTON_SELECTED
                } else {
                    BUTTON
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::SpawnPoint;

    fn menu_app() -> App {
        let level = Level::with_spawns(vec![SpawnPoint {
            identifier: "PlayerSpawn".into(),
            at: Vec2::ZERO,
        }]);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(bevy::state::app::StatesPlugin)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_state::<Run>()
            .insert_resource(LevelCatalog::for_test(vec![level.clone()]))
            .insert_resource(level)
            .add_plugins(DeveloperMenuPlugin);
        app.update();
        app
    }

    fn root_visibility(app: &mut App) -> Visibility {
        *app.world_mut()
            .query_filtered::<&Visibility, With<LevelMenuRoot>>()
            .single(app.world())
            .unwrap()
    }

    #[test]
    fn f1_opens_and_pauses_the_level_menu() {
        let mut app = menu_app();
        assert_eq!(root_visibility(&mut app), Visibility::Hidden);

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::F1);
        app.update();

        assert_eq!(root_visibility(&mut app), Visibility::Visible);
        assert!(app.world().resource::<Time<Virtual>>().is_paused());
    }

    #[test]
    fn clicking_a_level_closes_the_menu_and_requests_a_reload() {
        let mut app = menu_app();
        {
            let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            input.press(KeyCode::F1);
        }
        app.update();
        {
            let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            input.release(KeyCode::F1);
            input.clear();
        }

        let button = app
            .world_mut()
            .query_filtered::<Entity, With<LevelButton>>()
            .single(app.world())
            .unwrap();
        *app.world_mut().get_mut::<Interaction>(button).unwrap() = Interaction::Pressed;
        app.update();

        assert_eq!(root_visibility(&mut app), Visibility::Hidden);
        assert!(!app.world().resource::<Time<Virtual>>().is_paused());
        app.update();
        assert_eq!(*app.world().resource::<State<Run>>().get(), Run::Reloading);
    }
}
