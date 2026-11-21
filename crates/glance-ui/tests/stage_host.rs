//! The headless stage as a scripted host (v0.9.2): input events reach the UI, AccessKit's tree comes back, made-up
//! screens and the output windows follow the shell's rules and events, a quit is recorded. Needs a GPU; skips without
//! one, unless `GLANCE_REQUIRE_GPU=1`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use glance_ui::headless::Stage;
use glance_ui::{OutputEnd, Screen, ShellEvent};

fn screen(id: &str, name: &str) -> Screen {
    Screen { id: id.into(), name: name.into(), size: (1920, 1080), position: (0, 0), refresh_millihertz: Some(60_000), has_main_window: false }
}

fn stage() -> Option<Stage> {
    let g = glance_gpu::verify::test_gpu().unwrap()?;
    Some(Stage::new(&g))
}

const SIZE: egui::Vec2 = egui::Vec2::new(400.0, 300.0);

#[test]
fn a_click_reaches_the_ui_and_the_tree_names_the_button() {
    let Some(mut st) = stage() else { return };
    st.context().enable_accesskit();
    let mut clicks = 0;
    let mut at = egui::Pos2::ZERO;
    let run = |st: &mut Stage, events: Vec<egui::Event>, clicks: &mut i32, at: &mut egui::Pos2| {
        let input = egui::RawInput { events, ..Default::default() };
        st.step(SIZE, 2.0, input, |ui, _host| {
            egui::CentralPanel::default().show(ui, |ui| {
                let r = ui.button("Press me");
                *at = r.rect.center();
                if r.clicked() {
                    *clicks += 1;
                }
            });
        })
    };
    let out = run(&mut st, vec![], &mut clicks, &mut at);
    let update = out.platform_output.accesskit_update.expect("AccessKit on");
    assert!(update.nodes.iter().any(|(_, n)| n.label().is_some_and(|l| l == "Press me")), "the button is named in the tree");
    let pos = at;
    let press = |pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    run(&mut st, vec![egui::Event::PointerMoved(pos), press(true)], &mut clicks, &mut at);
    run(&mut st, vec![press(false)], &mut clicks, &mut at);
    assert_eq!(clicks, 1);
}

#[test]
fn outputs_follow_the_shells_rules_and_a_replug() {
    let Some(mut st) = stage() else { return };
    st.set_screens(vec![screen("1", "Main"), screen("2", "Projector")]);
    assert!(matches!(st.take_events().as_slice(), [ShellEvent::Screens(s)] if s.len() == 2));
    // Open on the projector by name; asking for a screen that is not there closes at once.
    st.with_host(|h| {
        h.open_output("Projector");
        h.add_output("Nowhere");
    });
    assert_eq!(st.outputs().iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["2"]);
    assert_eq!(st.take_events(), vec![ShellEvent::OutputClosed(OutputEnd::ScreenGone("Nowhere".into()))]);
    // The host tells the app where its outputs are.
    assert_eq!(st.with_host(|h| h.output_screen().map(|s| s.id.clone())), Some("2".into()));
    // Unplugged: the output closes with ScreenGone after the Screens event.
    st.set_screens(vec![screen("1", "Main")]);
    let ev = st.take_events();
    assert!(matches!(ev.as_slice(), [ShellEvent::Screens(_), ShellEvent::OutputClosed(OutputEnd::ScreenGone(id))] if id == "2"), "{ev:?}");
    assert!(st.outputs().is_empty());
    // Replugged and opened again; Open on another screen keeps only that one; Close closes all.
    st.set_screens(vec![screen("1", "Main"), screen("2", "Projector")]);
    st.with_host(|h| h.open_output("2"));
    st.with_host(|h| h.add_output("1"));
    assert_eq!(st.outputs().len(), 2);
    st.with_host(|h| h.open_output("1"));
    assert_eq!(st.outputs().iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["1"]);
    st.take_events();
    st.with_host(|h| h.close_output());
    assert_eq!(st.take_events(), vec![ShellEvent::OutputClosed(OutputEnd::Closed)]);
    assert!(!st.quit_asked());
    st.with_host(|h| h.quit());
    assert!(st.quit_asked());
}
