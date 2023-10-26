use crate::Enemy;
use rand::{thread_rng, Rng};
use std::f32::consts::PI;

use super::Projectile;
use super::SpriteHolder;

pub trait AI {
    fn ai_loop(&mut self, projectiles: &mut Vec<Projectile>, sprite_holder: &mut SpriteHolder, enemy: &Enemy);
}

pub struct Level0AI {

}

impl AI for Level0AI {
    fn ai_loop(&mut self, _projectiles: &mut Vec<Projectile>, _sprite_holder: &mut SpriteHolder, _enemy: &Enemy) {
        // Do nothing, used for Empty AI
    }
}

pub struct Level1AI {
    pub cooldown: usize,
    pub max_cooldown: usize,
}

impl AI for Level1AI {
    fn ai_loop(&mut self, projectiles: &mut Vec<Projectile>, sprite_holder: &mut SpriteHolder, enemy: &Enemy) {
        if self.cooldown > 0 {
            self.cooldown -= 1;
        }
        else {
            self.cooldown = self.max_cooldown;
            let angle: f32 = thread_rng().gen_range((11.0 * PI / 8.0)..=(13.0 * PI / 8.0));
            let velocity = (angle.cos() * 6.0, angle.sin() * 6.0);
            enemy.spawn_new_projectile(projectiles, sprite_holder, velocity);
        }
    }
}

pub struct Level6AI {
    pub cooldown: usize,
    pub max_cooldown: usize,
}

impl AI for Level6AI {
    fn ai_loop(&mut self, projectiles: &mut Vec<Projectile>, sprite_holder: &mut SpriteHolder, enemy: &Enemy) {
        self.cooldown += 1;
        if self.cooldown > 0 && self.cooldown <= 600 {
            if self.cooldown % 100 < 55 {
                let angle: f32 = (11.0 * PI / 8.0) + ((self.cooldown as f32) / 55.0).sin() * (3.0 * PI / 8.0);
                let velocity = (angle.cos() * 6.0, angle.sin() * 6.0);
                enemy.spawn_new_projectile(projectiles, sprite_holder, velocity);
            }
        }
        else if self.cooldown > 600 && self.cooldown <= 1200 {
            if self.cooldown % 30 == 0 {
                let mut angle: f32 = thread_rng().gen_range((9.0 * PI / 8.0)..=(11.0 * PI / 8.0));
                let velocity = (angle.cos() * 6.0, angle.sin() * 6.0);
                enemy.spawn_new_projectile(projectiles, sprite_holder, velocity);
                angle = angle + (2.0 * PI / 8.0);
                let velocity_2 = (angle.cos() * 6.0, angle.sin() * 6.0);
                enemy.spawn_new_projectile(projectiles, sprite_holder, velocity_2);
                angle = angle + (2.0 * PI / 8.0);
                let velocity_3 = (angle.cos() * 6.0, angle.sin() * 6.0);
                enemy.spawn_new_projectile(projectiles, sprite_holder, velocity_3);
            }
        }
        else if self.cooldown > 1200 && self.cooldown <= 1800 {
            if self.cooldown % 20 < 3 {
                let angle: f32 = (11.0 * PI / 8.0) + ((self.cooldown as f32) / 7.0).sin() * (3.0 * PI / 8.0);
                let velocity = (angle.cos() * 6.0, angle.sin() * 6.0);
                enemy.spawn_new_projectile(projectiles, sprite_holder, velocity);
            }
        }
    }
}