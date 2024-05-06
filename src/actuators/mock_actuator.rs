use std::time::Duration;
use sanscript_common::hid_actuator::HidActuator;

pub struct MockActuator {

}

impl MockActuator {
    pub fn new() -> MockActuator {
        MockActuator {}
    }
}

impl HidActuator for MockActuator {
    fn get_cursor_position(&self) -> (u16, u16) {
        (0, 0)
    }

    fn move_cursor(&mut self, x: i16, y: i16) {
        println!("Moving cursor");
    }

    fn mouse_down(&mut self, button: u8) {
        println!("Mouse down");
    }

    fn mouse_up(&mut self) {
        println!("Mouse up");
    }

    fn scroll_mouse_wheel(&mut self, x: i16, y: i16) {
        println!("Mouse wheel scrolled");
    }

    fn key_down(&mut self, key: &Vec<u8>) {
        println!("Key down");
    }

    fn clear_keys(&mut self) {
        println!("Cleared keys");
    }

    fn sleep(&mut self, duration_ms: usize) {
        std::thread::sleep(Duration::from_millis(duration_ms as u64));
    }
}