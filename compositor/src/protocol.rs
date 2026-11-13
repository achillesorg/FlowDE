//! JSON-lines protocol (one object per line).
//!
//! compositor -> shell (shell's socket, `$FLOWDE_SHELL_SOCKET`, default `$XDG_RUNTIME_DIR/flowde-shell.sock`):
//!   {"event":"gesture","name":"home|switcher|back|shade|qs"}
//!   {"event":"rotation","deg":0|90|180|270,"width":W,"height":H}
//!
//! test/tools -> compositor (`$FLOWDE_COMPOSITOR_SOCKET`, default `$XDG_RUNTIME_DIR/flowde-compositor.sock`):
//!   {"cmd":"touch","phase":"down|move|up","x":..,"y":..,"t":ms}
//!   {"cmd":"swipe","from":[x,y],"to":[x,y],"ms":300}      synthetic timestamps, deterministic
//!   {"cmd":"rotate","deg":90}
//!   {"cmd":"spawn","exec":"foot"}
//!   {"cmd":"state"}                                        -> {"mode":"shell|app","size":[w,h],"deg":..,"windows":n}
use serde_json::{json, Value};

pub fn gesture_event(name: &str) -> String {
    json!({"event":"gesture","name":name}).to_string()
}
pub fn rotation_event(deg: u32, w: i32, h: i32) -> String {
    json!({"event":"rotation","deg":deg,"width":w,"height":h}).to_string()
}
pub fn parse(line: &str) -> Option<Value> {
    serde_json::from_str(line.trim()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        let v = parse(&gesture_event("home")).unwrap();
        assert_eq!(v["name"], "home");
        assert!(parse("garbage").is_none());
    }
}
