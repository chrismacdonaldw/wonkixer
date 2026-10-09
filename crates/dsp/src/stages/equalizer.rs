use super::{FREQUENCY_SMOOTH_MS, LEVEL_SMOOTH_MS, Stage};
use crate::biquad::{Biquad, Coefficients};
use crate::params::{ParamId, ParamValues};
use crate::smoother::Smoother;

const SHELF_Q: f32 = 1.0;

#[derive(Clone, Copy)]
enum Shape {
    LowShelf,
    Peaking,
    HighShelf,
}

struct Band {
    shape: Shape,
    frequency_id: ParamId,
    gain_id: ParamId,
    q_id: Option<ParamId>,
    frequency: Smoother,
    gain: Smoother,
    q: Smoother,
    filter: Biquad,
}

impl Band {
    fn new(
        shape: Shape,
        frequency_id: ParamId,
        gain_id: ParamId,
        q_id: Option<ParamId>,
        sample_rate: f32,
    ) -> Self {
        let frequency = Smoother::new(
            frequency_id.spec().default,
            FREQUENCY_SMOOTH_MS,
            sample_rate,
        );
        let gain = Smoother::new(gain_id.spec().default, LEVEL_SMOOTH_MS, sample_rate);
        let q = Smoother::new(
            q_id.map_or(SHELF_Q, |id| id.spec().default),
            FREQUENCY_SMOOTH_MS,
            sample_rate,
        );
        let filter = Biquad::new(Self::coefficients(
            shape,
            sample_rate,
            frequency.value(),
            gain.value(),
            q.value(),
        ));
        Self {
            shape,
            frequency_id,
            gain_id,
            q_id,
            frequency,
            gain,
            q,
            filter,
        }
    }

    fn apply(&mut self, values: &ParamValues) {
        self.frequency.set_target(values.get(self.frequency_id));
        self.gain.set_target(values.get(self.gain_id));
        if let Some(id) = self.q_id {
            self.q.set_target(values.get(id));
        }
    }

    fn process(&mut self, sample_rate: f32, sample: f32) -> f32 {
        let moving =
            !(self.frequency.is_settled() && self.gain.is_settled() && self.q.is_settled());
        if moving {
            self.filter.set(Self::coefficients(
                self.shape,
                sample_rate,
                self.frequency.advance(),
                self.gain.advance(),
                self.q.advance(),
            ));
        }
        self.filter.process(sample)
    }

    fn coefficients(
        shape: Shape,
        sample_rate: f32,
        frequency: f32,
        gain_db: f32,
        q: f32,
    ) -> Coefficients {
        match shape {
            Shape::LowShelf => Coefficients::low_shelf(sample_rate, frequency, gain_db),
            Shape::Peaking => Coefficients::peaking(sample_rate, frequency, q, gain_db),
            Shape::HighShelf => Coefficients::high_shelf(sample_rate, frequency, gain_db),
        }
    }
}

pub struct Equalizer {
    sample_rate: f32,
    bands: [Band; 4],
}

impl Equalizer {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            bands: [
                Band::new(
                    Shape::LowShelf,
                    ParamId::LowShelfFrequency,
                    ParamId::LowShelfGain,
                    None,
                    sample_rate,
                ),
                Band::new(
                    Shape::Peaking,
                    ParamId::LowMidFrequency,
                    ParamId::LowMidGain,
                    Some(ParamId::LowMidQ),
                    sample_rate,
                ),
                Band::new(
                    Shape::Peaking,
                    ParamId::PresenceFrequency,
                    ParamId::PresenceGain,
                    Some(ParamId::PresenceQ),
                    sample_rate,
                ),
                Band::new(
                    Shape::HighShelf,
                    ParamId::HighShelfFrequency,
                    ParamId::HighShelfGain,
                    None,
                    sample_rate,
                ),
            ],
        }
    }
}

impl Stage for Equalizer {
    fn apply(&mut self, values: &ParamValues) {
        for band in &mut self.bands {
            band.apply(values);
        }
    }

    fn process(&mut self, sample: f32) -> f32 {
        self.bands.iter_mut().fold(sample, |signal, band| {
            band.process(self.sample_rate, signal)
        })
    }

    fn reset(&mut self) {
        for band in &mut self.bands {
            band.filter.reset();
        }
    }
}
