use san_vm::{runner};
use san_vm::actuators::mock_actuator::MockActuator;

fn main() {
    runner::run(MockActuator::new());
}