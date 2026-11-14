use super::*;

#[test]
fn a_fatal_report_quarantines_the_plugin_once() {
    let mut quarantine = Quarantine::default();
    assert!(quarantine.report_fatal(1, 10));
    assert!(quarantine.contains(&1));
    assert!(!quarantine.report_fatal(1, 10));
}

#[test]
fn reload_releases_the_plugin() {
    let mut quarantine = Quarantine::default();
    quarantine.report_fatal(1, 10);
    quarantine.begin_generation(1, 11);
    assert!(!quarantine.contains(&1));
}

#[test]
fn a_late_report_from_the_replaced_instance_does_not_quarantine_its_successor() {
    let mut quarantine = Quarantine::default();
    // the reload request is handled while the old instance's callback is still running
    quarantine.begin_generation(1, 11);
    assert!(!quarantine.report_fatal(1, 10));
    assert!(!quarantine.contains(&1));
    // the successor can still be quarantined for its own errors
    assert!(quarantine.report_fatal(1, 11));
    assert!(quarantine.contains(&1));
}

#[test]
fn generations_are_tracked_per_plugin() {
    let mut quarantine = Quarantine::default();
    quarantine.begin_generation(1, 11);
    assert!(quarantine.report_fatal(2, 10));
}

#[test]
fn a_forgotten_plugin_leaves_no_state_behind() {
    let mut quarantine = Quarantine::default();
    quarantine.begin_generation(1, 11);
    quarantine.report_fatal(1, 11);
    quarantine.forget(1);
    assert!(!quarantine.contains(&1));
    assert!(quarantine.oldest_current_generation.is_empty());
}
