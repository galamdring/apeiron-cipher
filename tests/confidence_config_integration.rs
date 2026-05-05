use apeiron_cipher::observation::{ConfidenceConfig, ObservationPlugin};
use bevy::prelude::*;

#[test]
fn confidence_config_loads_from_file() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, ObservationPlugin));

    // Run the startup systems
    app.update();

    // Check that the config resource was loaded
    let config = app.world().resource::<ConfidenceConfig>();

    // Verify the values from our test config file
    assert_eq!(config.death_degradation_factor, 0.6);
    assert_eq!(config.death_floor, 0.2);
    assert_eq!(config.domain_recovery_multiplier, 2.0);
    assert_eq!(config.passive_recovery_multiplier, 0.7);
    assert_eq!(config.base_observation_weight, 0.2);
}
