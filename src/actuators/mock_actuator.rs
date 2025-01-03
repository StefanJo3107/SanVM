use std::time::Duration;
use san_common::hid_actuator::HidActuator;

pub struct MockActuator {

}

impl MockActuator {
    pub fn new() -> MockActuator {
        MockActuator {}
    }
}

impl HidActuator for MockActuator {
    fn move_cursor(&mut self, x: i8, y: i8) {
        println!("Moving cursor to ({}, {})", x, y);
    }

    fn mouse_down(&mut self, _: u8) {
        println!("Mouse down");
    }

    fn mouse_up(&mut self) {
        println!("Mouse up");
    }

    fn scroll_mouse_wheel(&mut self, _: i8, _: i8) {
        println!("Mouse wheel scrolled");
    }

    fn key_down(&mut self, _: &Vec<u8>) {
        println!("Key down");
    }

    fn clear_keys(&mut self) {
        println!("Cleared keys");
    }

    fn sleep(&mut self, duration_ms: usize) {
        std::thread::sleep(Duration::from_millis(duration_ms as u64));
    }
}