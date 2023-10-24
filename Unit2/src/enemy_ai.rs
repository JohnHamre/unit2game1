use crate::Enemy;

use super::Projectile;
use super::SpriteHolder;

pub trait AI {
    fn ai_loop(&mut self, projectiles: &mut Vec<Projectile>, sprite_holder: &mut SpriteHolder, enemy: &Enemy);
}

pub struct Level0AI {

}

impl AI for Level0AI {
    fn ai_loop(&mut self, projectiles: &mut Vec<Projectile>, sprite_holder: &mut SpriteHolder, enemy: &Enemy) {
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
            enemy.spawn_new_projectile(projectiles, sprite_holder);
        }
    }
}