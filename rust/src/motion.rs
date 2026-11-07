//! A critically damped spring retargeted from the current presentation value.
#[derive(Clone, Copy, Debug)]
pub struct Spring {
    pub value: [f64; 4],
    pub velocity: [f64; 4],
    pub target: [f64; 4],
}
impl Spring {
    pub fn new(value: [f64; 4], target: [f64; 4]) -> Self {
        Self {
            value,
            target,
            velocity: [0.; 4],
        }
    }
    pub fn step(&mut self, dt: f64) -> bool {
        let dt = dt.clamp(0., 0.05);
        let omega = 30.;
        let decay = (-omega * dt).exp();
        for i in 0..4 {
            let distance = self.value[i] - self.target[i];
            let momentum = self.velocity[i] + omega * distance;
            self.value[i] = self.target[i] + (distance + momentum * dt) * decay;
            self.velocity[i] = (self.velocity[i] - omega * momentum * dt) * decay;
        }
        let settled = (0..4)
            .all(|i| (self.value[i] - self.target[i]).abs() < 0.5 && self.velocity[i].abs() < 6.);
        if settled {
            self.value = self.target;
            self.velocity = [0.; 4];
        }
        settled
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spring_moves_without_overshoot_and_settles() {
        let mut spring = Spring::new([0., 0., 800., 600.], [100., 50., 1920., 1080.]);
        assert!(!spring.step(1. / 60.));
        assert!(spring.value[0] > 0. && spring.value[0] < 100.);
        for _ in 0..90 {
            spring.step(1. / 60.);
        }
        assert!(spring.step(1. / 60.));
        assert_eq!(spring.value, spring.target);
    }
    #[test]
    fn reversal_keeps_current_position_and_velocity() {
        let mut spring = Spring::new([0.; 4], [100.; 4]);
        spring.step(1. / 60.);
        let current = spring.value;
        let velocity = spring.velocity;
        spring.target = [-100.; 4];
        assert_eq!(spring.value, current);
        assert_eq!(spring.velocity, velocity);
        for _ in 0..90 {
            spring.step(1. / 60.);
        }
        assert_eq!(spring.value, [-100.; 4]);
    }
}
