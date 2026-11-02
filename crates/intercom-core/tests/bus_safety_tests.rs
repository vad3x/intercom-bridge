use std::cell::Cell;

use intercom_core::domain::intercom::BusLineState;
use intercom_core::ports::{BusSafetyDriver, BusSwitchDriver};
use intercom_core::service::BusSafetyInterlock;

struct BusSwitchDriverMock<'a>(&'a Cell<BusLineState>);

impl BusSwitchDriver for BusSwitchDriverMock<'_> {
    fn set_bus_state(&mut self, state: BusLineState) {
        self.0.set(state);
    }
}

#[test]
fn inhibit_synchronously_clears_each_energized_relay() {
    // Arrange
    for initial in [
        BusLineState::Listen,
        BusLineState::Talk,
        BusLineState::DoorUnlock,
    ] {
        let physical_state = Cell::new(initial);
        let interlock = BusSafetyInterlock::new(BusSwitchDriverMock(&physical_state));

        // Act
        interlock.inhibit();

        // Assert
        assert_eq!(physical_state.get(), BusLineState::Idle);
    }
}

#[test]
fn inhibit_blocks_all_later_relay_changes_until_reboot() {
    // Arrange
    let physical_state = Cell::new(BusLineState::Idle);
    let interlock = BusSafetyInterlock::new(BusSwitchDriverMock(&physical_state));
    interlock.inhibit();

    // Act
    let observed = [
        BusLineState::Listen,
        BusLineState::Talk,
        BusLineState::DoorUnlock,
    ]
    .map(|requested| {
        (&interlock).set_bus_state(requested);
        physical_state.get()
    });

    // Assert
    assert_eq!(observed, [BusLineState::Idle; 3]);
}

#[test]
fn interlock_preserves_normal_bus_switches_before_inhibition() {
    // Arrange
    let physical_state = Cell::new(BusLineState::Idle);
    let interlock = BusSafetyInterlock::new(BusSwitchDriverMock(&physical_state));
    let requests = [
        BusLineState::Listen,
        BusLineState::Talk,
        BusLineState::DoorUnlock,
        BusLineState::Idle,
    ];

    // Act
    let observed = requests.map(|requested| {
        (&interlock).set_bus_state(requested);
        physical_state.get()
    });

    // Assert
    assert_eq!(observed, requests);
}
