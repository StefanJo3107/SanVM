use san_vm::{runner};
use san_vm::mock_actuator::MockActuator;

fn main() {
    runner::run(MockActuator{});
}