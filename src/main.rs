mod animation;
mod camera;
mod character;
mod combat;
mod enemy;
mod level;
mod player;
mod run;
mod shrine;
mod world;

use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(ImagePlugin::default_nearest())
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Slasher".into(),
                        resolution: (1280, 720).into(),
                        ..default()
                    }),
                    ..default()
                }),
        )
        .insert_resource(ClearColor(Color::srgb(0.13, 0.14, 0.20)))
        .add_plugins((
            camera::CameraPlugin,
            world::WorldPlugin,
            run::RunPlugin,
            level::LevelPlugin,
            character::CharacterPlugin,
            combat::CombatPlugin,
            player::PlayerPlugin,
            enemy::EnemyPlugin,
            shrine::ShrinePlugin,
            animation::AnimationPlugin,
        ))
        .run();
}
