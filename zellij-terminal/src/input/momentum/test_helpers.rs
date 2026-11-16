use super::*;

impl Coast {
    pub(super) fn speed_now(&self) -> f64 {
        self.speed * (-self.friction * self.reached).exp()
    }
}

impl Momentum {
    pub fn current_speed(&self) -> Option<f64> {
        self.coast.as_ref().map(Coast::speed_now)
    }
}
