use sanscript_common::hid_actuator::HidActuator;

pub struct MockActuator {

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

    fn key_down(&mut self, key: &[u8; 6]) {
        println!("Key down");
    }

    fn clear_keys(&mut self) {
        println!("Cleared keys");
    }
}