pub const PROBE_INTERVAL_TICKS: u32 = 60;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Route {
    pub input_id: String,
    pub output_id: String,
}

#[derive(Debug)]
pub struct Supervisor {
    enabled: bool,
    route: Option<Route>,
    idle_ticks: u32,
}

impl Default for Supervisor {
    fn default() -> Self {
        Self {
            enabled: true,
            route: None,
            idle_ticks: 0,
        }
    }
}

impl Supervisor {
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.idle_ticks = 0;
    }

    pub fn remember(&mut self, route: Route) {
        self.route = Some(route);
        self.idle_ticks = 0;
    }

    pub fn forget(&mut self) {
        self.route = None;
        self.idle_ticks = 0;
    }

    pub fn is_waiting(&self, live: bool) -> bool {
        self.enabled && !live && self.route.is_some()
    }

    pub fn tick(&mut self, live: bool) -> Option<&Route> {
        if !self.is_waiting(live) {
            self.idle_ticks = 0;
            return None;
        }
        self.idle_ticks += 1;
        if self.idle_ticks < PROBE_INTERVAL_TICKS {
            return None;
        }
        self.idle_ticks = 0;
        self.route.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route() -> Route {
        Route {
            input_id: "mic".to_owned(),
            output_id: "cable".to_owned(),
        }
    }

    fn probes_within(supervisor: &mut Supervisor, ticks: u32, live: bool) -> usize {
        (0..ticks)
            .filter(|_| supervisor.tick(live).is_some())
            .count()
    }

    #[test]
    fn probes_the_remembered_route_every_interval_while_it_is_down() {
        let mut supervisor = Supervisor::default();
        supervisor.remember(route());

        for _ in 1..PROBE_INTERVAL_TICKS {
            assert_eq!(supervisor.tick(false), None);
        }
        assert_eq!(supervisor.tick(false), Some(&route()));
        assert_eq!(
            probes_within(&mut supervisor, 3 * PROBE_INTERVAL_TICKS, false),
            3
        );
        assert!(supervisor.is_waiting(false));
    }

    #[test]
    fn never_probes_while_live() {
        let mut supervisor = Supervisor::default();
        supervisor.remember(route());

        assert_eq!(
            probes_within(&mut supervisor, 5 * PROBE_INTERVAL_TICKS, true),
            0
        );
        assert!(!supervisor.is_waiting(true));
    }

    #[test]
    fn a_stop_by_the_user_forgets_the_route() {
        let mut supervisor = Supervisor::default();
        supervisor.remember(route());
        supervisor.forget();

        assert_eq!(
            probes_within(&mut supervisor, 5 * PROBE_INTERVAL_TICKS, false),
            0
        );
        assert!(!supervisor.is_waiting(false));
    }

    #[test]
    fn nothing_is_probed_before_a_route_is_known() {
        let mut supervisor = Supervisor::default();

        assert_eq!(
            probes_within(&mut supervisor, 5 * PROBE_INTERVAL_TICKS, false),
            0
        );
    }

    #[test]
    fn disabling_pauses_recovery_without_losing_the_route() {
        let mut supervisor = Supervisor::default();
        supervisor.remember(route());
        supervisor.set_enabled(false);

        assert_eq!(
            probes_within(&mut supervisor, 5 * PROBE_INTERVAL_TICKS, false),
            0
        );
        assert!(!supervisor.is_waiting(false));

        supervisor.set_enabled(true);
        assert!(supervisor.is_waiting(false));
        assert_eq!(
            probes_within(&mut supervisor, PROBE_INTERVAL_TICKS, false),
            1
        );
    }

    #[test]
    fn going_live_restarts_the_interval() {
        let mut supervisor = Supervisor::default();
        supervisor.remember(route());

        probes_within(&mut supervisor, PROBE_INTERVAL_TICKS - 1, false);
        supervisor.tick(true);
        assert_eq!(
            probes_within(&mut supervisor, PROBE_INTERVAL_TICKS - 1, false),
            0
        );
        assert!(supervisor.tick(false).is_some());
    }
}
