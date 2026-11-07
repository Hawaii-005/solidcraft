//! Synthetic test reproduction of Touch & S Pen sketching pitfalls (Phase 1).
//! These tests assert the expected behavior for touch-aware sketch confirmation
//! and verify where the current desktop-inherited implementation fails.

use crate::scenario::Harness;
use serde_json::json;

fn harness(command: &str) -> Harness {
    let mut h = Harness::new();
    h.app.preferences.touch_sketch_confirm = true;
    h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
    h.app.cam.distance = 120.0;
    h.settle();
    h.step(&json!({"start": command})).unwrap();
    h
}

fn curves(h: &Harness) -> usize {
    let sid = h.app.session.active_sketch.unwrap();
    h.app.session.model.state().sketch(sid).unwrap().sketch.curves.len()
}

fn screen_pos(h: &mut Harness, sketch_pt: [f64; 2]) -> egui::Pos2 {
    let p = h.call("ui.at", json!({"sketch": sketch_pt}))["result"].clone();
    egui::pos2(p[0].as_f64().unwrap() as f32, p[1].as_f64().unwrap() as f32)
}

fn touch_tap(h: &mut Harness, pos: egui::Pos2, id: u64) {
    let tid = egui::TouchId(id);
    let dev = egui::TouchDeviceId(0);
    h.frame_with(vec![
        egui::Event::PointerMoved(pos),
        egui::Event::Touch {
            device_id: dev,
            id: tid,
            phase: egui::TouchPhase::Start,
            pos,
            force: None,
        },
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    ]);
    h.frame_with(vec![
        egui::Event::Touch {
            device_id: dev,
            id: tid,
            phase: egui::TouchPhase::End,
            pos,
            force: None,
        },
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::PointerGone,
    ]);
    h.settle();
}

/// Test 1: S Pen hover without contact after placing 1st point.
/// Must never create sketch geometry, only update hover preview.
#[test]
fn hover_after_first_point_creates_no_sketch() {
    let mut h = harness("sketch.rectangle.two_point");
    let pos0 = screen_pos(&mut h, [0.0, 0.0]);
    let pos1 = screen_pos(&mut h, [30.0, 20.0]);

    // Place 1st point via tap
    touch_tap(&mut h, pos0, 1);
    assert_eq!(curves(&h), 0);
    assert_eq!(
        h.app.tool.as_ref().map(|t| t.pts.len()),
        Some(1),
        "Tool must have exactly 1 point after first tap"
    );

    // S Pen hovers without contact to Point B (PointerMoved puro, sem touch e sem botões)
    h.call("ui.move", json!({"x": pos1.x, "y": pos1.y}));

    // No geometry should be created on hover alone
    assert_eq!(curves(&h), 0, "Hover must not create any sketch geometry");
    assert_eq!(h.app.tool.as_ref().unwrap().pts.len(), 1, "Tool must still be waiting for 2nd point");
}

/// Test 2: Touch on Point A, then Touch on Point B.
/// On touch devices with confirm preference, touching point B must stage/preview
/// without committing curves immediately.
/// (FAILS CURRENTLY: SolidCraft immediately creates 4 curves on the 2nd tap)
#[test]
fn two_point_touch_stages_preview_without_creating_geometry() {
    let mut h = harness("sketch.rectangle.two_point");
    let pos0 = screen_pos(&mut h, [0.0, 0.0]);
    let pos1 = screen_pos(&mut h, [30.0, 20.0]);

    // Tap point A
    touch_tap(&mut h, pos0, 1);
    assert_eq!(curves(&h), 0);

    // Tap point B
    touch_tap(&mut h, pos1, 2);
    h.settle();

    // In the proposed touch-confirm architecture, 0 curves should exist until confirmed.
    // FAILS in current SolidCraft: curves(&h) is 4.
    assert_eq!(
        curves(&h),
        0,
        "Tap on point B should only stage the rectangle preview, not commit immediately"
    );
}

/// Test 3: Floating confirm (✓) and cancel (✕) buttons.
/// Confirm commits the staged shape; cancel discards it.
/// (FAILS CURRENTLY: no floating touch confirm/cancel buttons exist)
#[test]
fn touch_sketch_confirm_button_creates_and_cancel_discards() {
    // 1. Confirm test
    {
        let mut h = harness("sketch.rectangle.two_point");
        let pos0 = screen_pos(&mut h, [0.0, 0.0]);
        let pos1 = screen_pos(&mut h, [30.0, 20.0]);

        touch_tap(&mut h, pos0, 1);
        touch_tap(&mut h, pos1, 2);
        assert!(h.app.tool.as_ref().unwrap().awaiting_confirmation, "Tool must be awaiting confirmation");

        h.frame();
        let rects = crate::tools::touch_confirm_rects(&h.app);
        assert_eq!(rects.len(), 2, "Both confirm (✓) and cancel (✕) buttons must have valid touch bounds");
        assert_eq!(rects[0].size(), egui::vec2(48.0, 48.0), "Confirm button must be 48x48 dp");
        assert_eq!(rects[1].size(), egui::vec2(48.0, 48.0), "Cancel button must be 48x48 dp");

        // Click confirm (✓) button via real ui.click
        let confirm_center = rects[0].center();
        h.call("ui.move", json!({"x": confirm_center.x, "y": confirm_center.y}));
        h.call("ui.click", json!({"x": confirm_center.x, "y": confirm_center.y}));
        h.frame();
        h.settle();

        assert_eq!(curves(&h), 4, "Confirming button must create the rectangle curves");
        assert!(h.app.tool.as_ref().is_some_and(|t| t.pts.is_empty()), "Tool points must be cleared after confirm");
    }

    // 2. Cancel test
    {
        let mut h = harness("sketch.rectangle.two_point");
        let pos0 = screen_pos(&mut h, [0.0, 0.0]);
        let pos1 = screen_pos(&mut h, [30.0, 20.0]);

        touch_tap(&mut h, pos0, 1);
        touch_tap(&mut h, pos1, 2);
        assert!(h.app.tool.as_ref().unwrap().awaiting_confirmation);

        let rects = crate::tools::touch_confirm_rects(&h.app);
        let cancel_center = rects[1].center();

        // Click cancel (✕) button via real ui.click
        h.call("ui.move", json!({"x": cancel_center.x, "y": cancel_center.y}));
        h.call("ui.click", json!({"x": cancel_center.x, "y": cancel_center.y}));
        h.frame();
        h.settle();

        assert_eq!(curves(&h), 0, "Cancelling must discard staged rectangle without creating curves");
        assert_eq!(h.app.tool.as_ref().unwrap().pts.len(), 1, "Cancelling must keep the first point");
    }
}

/// Test 4: Two-finger pinch must not place sketch points or mutate geometry.
/// (FAILS CURRENTLY: first finger landing places a point via two_point_start)
#[test]
fn two_finger_pinch_does_not_place_sketch_points_or_mutate_geometry() {
    let mut h = harness("sketch.rectangle.two_point");
    let pos0 = screen_pos(&mut h, [0.0, 0.0]);
    let pos1 = screen_pos(&mut h, [40.0, 30.0]);

    let dev = egui::TouchDeviceId(0);
    // Finger 1 lands
    h.frame_with(vec![
        egui::Event::Touch {
            device_id: dev,
            id: egui::TouchId(1),
            phase: egui::TouchPhase::Start,
            pos: pos0,
            force: None,
        },
        egui::Event::PointerMoved(pos0),
        egui::Event::PointerButton {
            pos: pos0,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    ]);

    // Finger 2 lands (pinch starts, natural touch_multi derivation)
    h.frame_with(vec![
        egui::Event::Touch {
            device_id: dev,
            id: egui::TouchId(2),
            phase: egui::TouchPhase::Start,
            pos: pos1,
            force: None,
        },
    ]);

    // Both fingers move (drag / pinch gesture without synthetic Zoom event)
    let pos0_moved = pos0 + egui::vec2(-10.0, -10.0);
    let pos1_moved = pos1 + egui::vec2(15.0, 15.0);
    h.frame_with(vec![
        egui::Event::Touch {
            device_id: dev,
            id: egui::TouchId(1),
            phase: egui::TouchPhase::Move,
            pos: pos0_moved,
            force: None,
        },
        egui::Event::Touch {
            device_id: dev,
            id: egui::TouchId(2),
            phase: egui::TouchPhase::Move,
            pos: pos1_moved,
            force: None,
        },
    ]);

    // Both lift
    h.frame_with(vec![
        egui::Event::Touch {
            device_id: dev,
            id: egui::TouchId(1),
            phase: egui::TouchPhase::End,
            pos: pos0,
            force: None,
        },
        egui::Event::Touch {
            device_id: dev,
            id: egui::TouchId(2),
            phase: egui::TouchPhase::End,
            pos: pos1,
            force: None,
        },
        egui::Event::PointerButton {
            pos: pos0,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::PointerGone,
    ]);
    h.settle();

    // A pinch must never leave points in the sketch tool or create curves
    assert!(
        h.app.tool.as_ref().is_some_and(|t| t.pts.is_empty()),
        "Pinch gesture must not add points to the active drawing tool"
    );
    assert_eq!(curves(&h), 0, "Pinch gesture must not create any geometry");
}

/// Test 5 (Ajuste 1): Draw-drag com toque/caneta vs mouse.
/// Pousar toque no ponto 1, arrastar > 24 pt e soltar.
/// - Com toque (Touch events): ponto 2 fica em staging, prévia aparece, 0 curvas criadas.
/// - Com mouse (PointerButton puro): cria as 4 curvas imediatamente ao soltar (comportamento desktop).
/// (FAILS CURRENTLY: com toque, o SolidCraft cria 4 curvas imediatamente ao soltar!)
#[test]
fn draw_drag_touch_stages_preview_while_mouse_commits() {
    // Caso A: Toque / Caneta no vidro
    let mut h_touch = harness("sketch.rectangle.two_point");
    let pos0 = screen_pos(&mut h_touch, [0.0, 0.0]);
    let pos1 = screen_pos(&mut h_touch, [30.0, 20.0]); // > 24 pt de arrasto
    let dev = egui::TouchDeviceId(0);
    let tid = egui::TouchId(10);

    // Touch down no ponto 0
    h_touch.frame_with(vec![
        egui::Event::Touch {
            device_id: dev,
            id: tid,
            phase: egui::TouchPhase::Start,
            pos: pos0,
            force: None,
        },
        egui::Event::PointerMoved(pos0),
        egui::Event::PointerButton {
            pos: pos0,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    ]);

    // Arrastar até pos1 (> 24 pt)
    h_touch.frame_with(vec![
        egui::Event::Touch {
            device_id: dev,
            id: tid,
            phase: egui::TouchPhase::Move,
            pos: pos1,
            force: None,
        },
        egui::Event::PointerMoved(pos1),
    ]);

    // Soltar toque em pos1
    h_touch.frame_with(vec![
        egui::Event::Touch {
            device_id: dev,
            id: tid,
            phase: egui::TouchPhase::End,
            pos: pos1,
            force: None,
        },
        egui::Event::PointerButton {
            pos: pos1,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::PointerGone,
    ]);
    h_touch.settle();

    // Esperado com a nova regra de toque: 0 curvas criadas, aguardando confirmação.
    // FALHA HOJE: cria 4 curvas ao soltar!
    assert_eq!(
        curves(&h_touch),
        0,
        "Draw-drag com toque/caneta deve apenas posicionar o ponto 2 em staging, sem comitar imediatamente"
    );
    assert!(
        h_touch.app.tool.as_ref().is_some_and(|t| t.awaiting_confirmation),
        "Ferramenta deve entrar em estado awaiting_confirmation após soltar o arrasto de toque"
    );
}

/// Test 6: Desktop / Mouse connected maintains 2 clicks without staging.
/// Mouse clicks without touch events commit immediately even if touch_sketch_confirm is true.
#[test]
fn mouse_clicks_maintain_immediate_commit() {
    let mut h = harness("sketch.rectangle.two_point");
    // Ensure last_touch_event_at is None (pure mouse usage)
    h.app.viewport.last_touch_event_at = None;
    let pos0 = screen_pos(&mut h, [0.0, 0.0]);
    let pos1 = screen_pos(&mut h, [30.0, 20.0]);

    // Mouse click 1 (pure pointer button, no Touch event)
    h.call("ui.move", json!({"x": pos0.x, "y": pos0.y}));
    h.call("ui.click", json!({"x": pos0.x, "y": pos0.y}));
    assert_eq!(curves(&h), 0);
    assert_eq!(h.app.tool.as_ref().unwrap().pts.len(), 1);

    // Mouse click 2
    h.call("ui.move", json!({"x": pos1.x, "y": pos1.y}));
    h.call("ui.click", json!({"x": pos1.x, "y": pos1.y}));

    // Mouse must commit 4 curves directly
    assert_eq!(curves(&h), 4, "Mouse clicks must create geometry immediately");
    assert!(h.app.tool.as_ref().unwrap().pts.is_empty(), "Tool should be ready for next shape");
}

/// Mouse clicks after the touch window expires (>1.0s) commit immediately even if touch occurred earlier.
#[test]
fn mouse_clicks_after_touch_window_expired_maintain_immediate_commit() {
    let mut h = harness("sketch.rectangle.two_point");
    let pos0 = screen_pos(&mut h, [0.0, 0.0]);
    let pos1 = screen_pos(&mut h, [30.0, 20.0]);

    // Perform a real touch tap in the viewport
    touch_tap(&mut h, pos0, 1);
    // Cancel the touch tool to leave a clean state but keep the touch history
    h.app.start("sketch.rectangle.two_point");
    h.frame();

    // Advance time beyond the 1.0s touch window (e.g. 70 frames = 1.16s)
    h.frames(70);

    // Mouse click 1 (pure pointer button, no Touch event)
    h.call("ui.move", json!({"x": pos0.x, "y": pos0.y}));
    h.call("ui.click", json!({"x": pos0.x, "y": pos0.y}));
    assert_eq!(curves(&h), 0);
    assert_eq!(h.app.tool.as_ref().unwrap().pts.len(), 1);

    // Mouse click 2
    h.call("ui.move", json!({"x": pos1.x, "y": pos1.y}));
    h.call("ui.click", json!({"x": pos1.x, "y": pos1.y}));

    // Mouse commits 4 curves immediately without staging
    assert_eq!(curves(&h), 4, "Mouse outside touch window must commit directly");
    assert!(h.app.tool.as_ref().unwrap().pts.is_empty());
}

/// Test 7: Tapping on confirm ✓ / cancel ✕ button never places or moves points in viewport.
#[test]
fn clicking_confirm_or_cancel_never_picks_or_places_extra_points() {
    // 1. Cancel button test
    {
        let mut h = harness("sketch.rectangle.two_point");
        let pos0 = screen_pos(&mut h, [0.0, 0.0]);
        let pos1 = screen_pos(&mut h, [30.0, 20.0]);

        touch_tap(&mut h, pos0, 1);
        touch_tap(&mut h, pos1, 2);
        assert!(h.app.tool.as_ref().unwrap().awaiting_confirmation);

        let rects = crate::tools::touch_confirm_rects(&h.app);
        let cancel_center = rects[1].center();

        // Tap directly on the cancel button coordinates in the viewport
        h.call("ui.move", json!({"x": cancel_center.x, "y": cancel_center.y}));
        h.call("ui.click", json!({"x": cancel_center.x, "y": cancel_center.y}));
        h.frame();
        h.settle();

        // Must be cancelled: 0 curves, and tool.pts should have popped the 2nd point without adding cancel_center as a point
        assert_eq!(curves(&h), 0);
        assert_eq!(h.app.tool.as_ref().unwrap().pts.len(), 1, "Cancelling must discard staged point and keep first point");
        assert_eq!(h.app.tool.as_ref().unwrap().pts[0].0, [0.0, 0.0].into(), "First point must be untouched");
    }

    // 2. Confirm button test
    {
        let mut h = harness("sketch.rectangle.two_point");
        let pos0 = screen_pos(&mut h, [0.0, 0.0]);
        let pos1 = screen_pos(&mut h, [30.0, 20.0]);

        touch_tap(&mut h, pos0, 1);
        touch_tap(&mut h, pos1, 2);
        assert!(h.app.tool.as_ref().unwrap().awaiting_confirmation);

        let rects = crate::tools::touch_confirm_rects(&h.app);
        let confirm_center = rects[0].center();

        // Tap directly on the confirm button coordinates in the viewport
        h.call("ui.move", json!({"x": confirm_center.x, "y": confirm_center.y}));
        h.call("ui.click", json!({"x": confirm_center.x, "y": confirm_center.y}));
        h.frame();
        h.settle();

        // Must commit 4 curves, without adding confirm_center as a separate point
        assert_eq!(curves(&h), 4);
        assert!(h.app.tool.as_ref().unwrap().pts.is_empty(), "Confirming must clear points and commit");
    }
}

/// Third touch tap repositions the staged second point without committing or creating curves.
#[test]
fn third_touch_tap_repositions_staged_point() {
    let mut h = harness("sketch.rectangle.two_point");
    let pos0 = screen_pos(&mut h, [0.0, 0.0]);
    let pos1 = screen_pos(&mut h, [30.0, 20.0]);
    let pos2 = screen_pos(&mut h, [-15.0, -10.0]);

    // Tap 1: place point A
    touch_tap(&mut h, pos0, 1);
    assert_eq!(h.app.tool.as_ref().unwrap().pts.len(), 1);

    // Tap 2: stage point B at pos1
    touch_tap(&mut h, pos1, 2);
    assert!(h.app.tool.as_ref().unwrap().awaiting_confirmation);
    assert_eq!(curves(&h), 0);
    assert_eq!(h.app.tool.as_ref().unwrap().pts.len(), 2);
    assert_eq!(h.app.tool.as_ref().unwrap().pts[1].0, [30.0, 20.0].into());

    // Tap 3: tap at pos2 -> repositions point B without committing
    touch_tap(&mut h, pos2, 3);
    assert!(h.app.tool.as_ref().unwrap().awaiting_confirmation);
    assert_eq!(curves(&h), 0, "Repositioning 2nd point must not create any geometry");
    assert_eq!(h.app.tool.as_ref().unwrap().pts.len(), 2);
    assert_eq!(h.app.tool.as_ref().unwrap().pts[1].0, [-15.0, -10.0].into(), "Second point must be moved to pos2");
}

/// Pressing Enter confirms the staged shape, while pressing Escape cancels the staged point.
#[test]
fn keyboard_enter_confirms_and_escape_cancels_staged_shape() {
    // 1. Enter confirms
    {
        let mut h = harness("sketch.rectangle.two_point");
        let pos0 = screen_pos(&mut h, [0.0, 0.0]);
        let pos1 = screen_pos(&mut h, [30.0, 20.0]);

        touch_tap(&mut h, pos0, 1);
        touch_tap(&mut h, pos1, 2);
        assert!(h.app.tool.as_ref().unwrap().awaiting_confirmation);

        // Press Enter key
        h.frame_with(vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        h.settle();

        assert_eq!(curves(&h), 4, "Enter key must confirm staged shape");
        assert!(h.app.tool.as_ref().unwrap().pts.is_empty());
    }

    // 2. Escape cancels
    {
        let mut h = harness("sketch.rectangle.two_point");
        let pos0 = screen_pos(&mut h, [0.0, 0.0]);
        let pos1 = screen_pos(&mut h, [30.0, 20.0]);

        touch_tap(&mut h, pos0, 1);
        touch_tap(&mut h, pos1, 2);
        assert!(h.app.tool.as_ref().unwrap().awaiting_confirmation);

        // Press Escape key
        h.frame_with(vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        h.settle();

        assert_eq!(curves(&h), 0, "Escape key must cancel staged shape without curves");
        assert_eq!(h.app.tool.as_ref().unwrap().pts.len(), 1, "Escape must keep first point");
    }
}

/// Test staggered touch landing pinch:
/// Dedo A pousa com PointerMoved + PointerButton press (e variante com arrasto > DRAW_DRAG_MIN),
/// um frame depois dedo B pousa, ambos movem e soltam.
/// (a) Com 1 ponto já colocado: continua 1 ponto.
/// (b) Em staging: awaiting_confirmation continua true e 0 curvas.
#[test]
fn staggered_touch_pinch_preserves_points_and_staged_state() {
    let dev = egui::TouchDeviceId(0);

    // Caso (a): 1 ponto colocado, Dedo A pousa com PointerMoved + PointerButton press + arrasto > DRAW_DRAG_MIN,
    // um frame depois Dedo B pousa, ambos movem e soltam. Ponto 1 continua intacto.
    {
        let mut h = harness("sketch.rectangle.two_point");
        let pos0 = screen_pos(&mut h, [0.0, 0.0]);
        let finger_a1 = screen_pos(&mut h, [-15.0, -10.0]);
        let finger_a2 = finger_a1 + egui::vec2(25.0, 25.0); // arrasto > DRAW_DRAG_MIN (14.0)
        let finger_b1 = screen_pos(&mut h, [-25.0, -20.0]);
        assert!(!crate::shell::covered(&h.app, finger_a1));
        assert!(!crate::shell::covered(&h.app, finger_b1));

        touch_tap(&mut h, pos0, 1);
        assert_eq!(h.app.tool.as_ref().unwrap().pts.len(), 1, "Ponto 1 deve estar colocado");

        // Frame 1: Dedo A pousa e inicia arrasto
        h.frame_with(vec![
            egui::Event::Touch {
                device_id: dev,
                id: egui::TouchId(30),
                phase: egui::TouchPhase::Start,
                pos: finger_a1,
                force: None,
            },
            egui::Event::PointerMoved(finger_a1),
            egui::Event::PointerButton {
                pos: finger_a1,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerMoved(finger_a2),
        ]);

        // Frame 2: Um frame depois, Dedo B pousa (virando multi-toque)
        h.frame_with(vec![
            egui::Event::Touch {
                device_id: dev,
                id: egui::TouchId(31),
                phase: egui::TouchPhase::Start,
                pos: finger_b1,
                force: None,
            },
        ]);

        // Frame 3: Ambos os dedos movem (pinça / navegação)
        h.frame_with(vec![
            egui::Event::Touch {
                device_id: dev,
                id: egui::TouchId(30),
                phase: egui::TouchPhase::Move,
                pos: finger_a2 + egui::vec2(-10.0, -10.0),
                force: None,
            },
            egui::Event::Touch {
                device_id: dev,
                id: egui::TouchId(31),
                phase: egui::TouchPhase::Move,
                pos: finger_b1 + egui::vec2(10.0, 10.0),
                force: None,
            },
            egui::Event::PointerMoved(finger_a2 + egui::vec2(-10.0, -10.0)),
        ]);

        // Frame 4: Soltam
        h.frame_with(vec![
            egui::Event::Touch {
                device_id: dev,
                id: egui::TouchId(30),
                phase: egui::TouchPhase::End,
                pos: finger_a2,
                force: None,
            },
            egui::Event::Touch {
                device_id: dev,
                id: egui::TouchId(31),
                phase: egui::TouchPhase::End,
                pos: finger_b1,
                force: None,
            },
            egui::Event::PointerButton {
                pos: finger_a2,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerGone,
        ]);
        h.settle();

        assert_eq!(curves(&h), 0);
        assert_eq!(h.app.tool.as_ref().map(|t| t.pts.len()), Some(1), "Ponto 1 deve continuar intacto");

        // Sub-teste 1: pinça escalonada, soltar tudo, esperar 2 frames, depois toque normal deve colocar ponto
        h.frame();
        h.frame();
        let pos_next = screen_pos(&mut h, [-15.0, -10.0]);
        assert!(!crate::shell::covered(&h.app, pos_next), "pos_next não pode estar em covered_rects");
        touch_tap(&mut h, pos_next, 3);
        assert_eq!(h.app.tool.as_ref().map(|t| t.pts.len()), Some(2), "Toque normal 2 frames após pinça deve colocar ponto");

        // Sub-teste 2: mouse depois de pinça (2 cliques ainda confirmam imediatamente)
        let mut h_mouse = harness("sketch.rectangle.two_point");
        let m_pos0 = screen_pos(&mut h_mouse, [0.0, 0.0]);
        let m_pos1 = screen_pos(&mut h_mouse, [30.0, 20.0]);
        let pinch_a = screen_pos(&mut h_mouse, [40.0, 40.0]);
        let pinch_b = screen_pos(&mut h_mouse, [70.0, 70.0]);

        // Executar pinça com touch
        h_mouse.frame_with(vec![
            egui::Event::Touch { device_id: dev, id: egui::TouchId(50), phase: egui::TouchPhase::Start, pos: pinch_a, force: None },
            egui::Event::Touch { device_id: dev, id: egui::TouchId(51), phase: egui::TouchPhase::Start, pos: pinch_b, force: None },
        ]);
        h_mouse.frame_with(vec![
            egui::Event::Touch { device_id: dev, id: egui::TouchId(50), phase: egui::TouchPhase::End, pos: pinch_a, force: None },
            egui::Event::Touch { device_id: dev, id: egui::TouchId(51), phase: egui::TouchPhase::End, pos: pinch_b, force: None },
            egui::Event::PointerGone,
        ]);
        h_mouse.frames(70);

        // 2 cliques de mouse devem confirmar o retângulo diretamente (4 curvas)
        h_mouse.call("ui.move", json!({"x": m_pos0.x, "y": m_pos0.y}));
        h_mouse.call("ui.click", json!({"x": m_pos0.x, "y": m_pos0.y}));
        h_mouse.call("ui.move", json!({"x": m_pos1.x, "y": m_pos1.y}));
        h_mouse.call("ui.click", json!({"x": m_pos1.x, "y": m_pos1.y}));
        assert_eq!(curves(&h_mouse), 4, "Mouse após pinça deve confirmar de imediato");
    }

    // Caso (b): Em staging (2 pontos), Dedo A pousa, depois Dedo B pousa, ambos movem e soltam.
    // awaiting_confirmation continua true e 0 curvas.
    {
        let mut h = harness("sketch.rectangle.two_point");
        let pos0 = screen_pos(&mut h, [0.0, 0.0]);
        let pos1 = screen_pos(&mut h, [30.0, 20.0]);
        let finger_a1 = screen_pos(&mut h, [-15.0, -10.0]);
        let finger_a2 = finger_a1 + egui::vec2(20.0, 20.0);
        let finger_b1 = screen_pos(&mut h, [-25.0, -20.0]);
        assert!(!crate::shell::covered(&h.app, finger_a1));
        assert!(!crate::shell::covered(&h.app, finger_b1));

        touch_tap(&mut h, pos0, 1);
        touch_tap(&mut h, pos1, 2);
        assert!(h.app.tool.as_ref().unwrap().awaiting_confirmation);

        // Frame 1: Dedo A pousa
        h.frame_with(vec![
            egui::Event::Touch {
                device_id: dev,
                id: egui::TouchId(40),
                phase: egui::TouchPhase::Start,
                pos: finger_a1,
                force: None,
            },
            egui::Event::PointerMoved(finger_a1),
            egui::Event::PointerButton {
                pos: finger_a1,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerMoved(finger_a2),
        ]);

        // Frame 2: Dedo B pousa no frame seguinte
        h.frame_with(vec![
            egui::Event::Touch {
                device_id: dev,
                id: egui::TouchId(41),
                phase: egui::TouchPhase::Start,
                pos: finger_b1,
                force: None,
            },
        ]);

        // Frame 3: Movem
        h.frame_with(vec![
            egui::Event::Touch {
                device_id: dev,
                id: egui::TouchId(40),
                phase: egui::TouchPhase::Move,
                pos: finger_a2 + egui::vec2(-10.0, -10.0),
                force: None,
            },
            egui::Event::Touch {
                device_id: dev,
                id: egui::TouchId(41),
                phase: egui::TouchPhase::Move,
                pos: finger_b1 + egui::vec2(10.0, 10.0),
                force: None,
            },
        ]);

        // Frame 4: Soltam
        h.frame_with(vec![
            egui::Event::Touch {
                device_id: dev,
                id: egui::TouchId(40),
                phase: egui::TouchPhase::End,
                pos: finger_a2,
                force: None,
            },
            egui::Event::Touch {
                device_id: dev,
                id: egui::TouchId(41),
                phase: egui::TouchPhase::End,
                pos: finger_b1,
                force: None,
            },
            egui::Event::PointerButton {
                pos: finger_a2,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        h.settle();

        assert_eq!(curves(&h), 0, "Pinça escalonada não deve criar curvas");
        assert!(
            h.app.tool.as_ref().is_some_and(|t| t.awaiting_confirmation),
            "awaiting_confirmation deve continuar true após pinça escalonada"
        );
        assert_eq!(h.app.tool.as_ref().unwrap().pts.len(), 2);
    }
}

/// Tocar no ✓ e no ✕ usando touch_tap (Event::Touch + ponteiro) nas coordenadas dos botões:
/// nenhum ponto novo é colocado ou movido; ✓ cria 4 curvas, ✕ mantém 1 ponto.
#[test]
fn touch_tapping_confirm_and_cancel_buttons_acts_without_adding_points() {
    // 1. Tocar com touch_tap no ✓
    {
        let mut h = harness("sketch.rectangle.two_point");
        let pos0 = screen_pos(&mut h, [0.0, 0.0]);
        let pos1 = screen_pos(&mut h, [30.0, 20.0]);

        touch_tap(&mut h, pos0, 1);
        touch_tap(&mut h, pos1, 2);
        assert!(h.app.tool.as_ref().unwrap().awaiting_confirmation);

        let rects = crate::tools::touch_confirm_rects(&h.app);
        let confirm_center = rects[0].center();

        // Tocar com touch_tap real (Touch Start/End + Pointer events) no centro do ✓
        touch_tap(&mut h, confirm_center, 50);

        assert_eq!(curves(&h), 4, "Tocar no ✓ via touch_tap deve criar as 4 curvas");
        assert!(h.app.tool.as_ref().unwrap().pts.is_empty(), "Pontos devem ser limpos após comitar");
    }

    // 2. Tocar com touch_tap no ✕
    {
        let mut h = harness("sketch.rectangle.two_point");
        let pos0 = screen_pos(&mut h, [0.0, 0.0]);
        let pos1 = screen_pos(&mut h, [30.0, 20.0]);

        touch_tap(&mut h, pos0, 1);
        touch_tap(&mut h, pos1, 2);
        assert!(h.app.tool.as_ref().unwrap().awaiting_confirmation);

        let rects = crate::tools::touch_confirm_rects(&h.app);
        let cancel_center = rects[1].center();

        // Tocar com touch_tap real no centro do ✕
        touch_tap(&mut h, cancel_center, 51);

        assert_eq!(curves(&h), 0, "Tocar no ✕ via touch_tap não deve criar curvas");
        assert_eq!(h.app.tool.as_ref().unwrap().pts.len(), 1, "Ponto 1 deve ser mantido");
        assert_eq!(h.app.tool.as_ref().unwrap().pts[0].0, [0.0, 0.0].into(), "Ponto 1 não pode ter sido movido");
    }
}

// -----------------------------------------------------------------------------
// FASE 2: Testes para Linha Encadeada (Kind::Draw(0))
// -----------------------------------------------------------------------------

/// 1. Dois toques em A e B: 0 segmentos até ✓; ✓ cria 1 e a cadeia continua em B.
#[test]
fn line_chain_two_touch_taps_stages_preview_until_confirmed() {
    let mut h = harness("sketch.line");
    let pos0 = screen_pos(&mut h, [0.0, 0.0]);
    let pos1 = screen_pos(&mut h, [20.0, 15.0]);
    assert!(!crate::shell::covered(&h.app, pos0));
    assert!(!crate::shell::covered(&h.app, pos1));

    // 1º toque em A
    touch_tap(&mut h, pos0, 1);
    assert_eq!(curves(&h), 0);
    assert_eq!(h.app.tool.as_ref().unwrap().pts.len(), 1);

    // 2º toque em B
    touch_tap(&mut h, pos1, 2);
    // Deve estar em staging (0 curvas criadas até confirmação)
    assert_eq!(curves(&h), 0, "Dois toques não devem criar segmento imediatamente; deve aguardar confirmação");
    assert!(h.app.tool.as_ref().unwrap().awaiting_confirmation, "Ferramenta deve estar aguardando confirmação");

    let rects = crate::tools::touch_confirm_rects(&h.app);
    assert!(!rects.is_empty(), "Botões de confirmação devem estar visíveis");
    let confirm_center = rects[0].center();

    // Confirmar com ✓ via touch_tap
    touch_tap(&mut h, confirm_center, 10);
    assert_eq!(curves(&h), 1, "✓ deve confirmar o 1º segmento");

    // A cadeia continua ativa em B
    let tool = h.app.tool.as_ref().unwrap();
    assert_eq!(tool.cmd, "sketch.line");
    assert_eq!(tool.pts.len(), 1, "Cadeia deve continuar com o ponto B como novo início");
    assert_eq!(tool.pts[0].0, [20.0, 15.0].into());
    assert!(!tool.awaiting_confirmation, "Aguardando próximo ponto da cadeia");
}

/// 2. ✕ descarta só o pendente; segmentos já confirmados ficam.
#[test]
fn line_chain_cancel_discards_only_staged_segment_keeping_confirmed() {
    let mut h = harness("sketch.line");
    let pos0 = screen_pos(&mut h, [0.0, 0.0]);
    let pos1 = screen_pos(&mut h, [20.0, 15.0]);
    let pos2 = screen_pos(&mut h, [20.0, 30.0]);
    assert!(!crate::shell::covered(&h.app, pos0));
    assert!(!crate::shell::covered(&h.app, pos1));
    assert!(!crate::shell::covered(&h.app, pos2));

    // Segmento 1: A -> B
    touch_tap(&mut h, pos0, 1);
    touch_tap(&mut h, pos1, 2);
    assert!(h.app.tool.as_ref().unwrap().awaiting_confirmation);
    let confirm_center = crate::tools::touch_confirm_rects(&h.app)[0].center();
    touch_tap(&mut h, confirm_center, 10);
    assert_eq!(curves(&h), 1, "1º segmento confirmado");

    // Segmento 2 (em staging): B -> C
    touch_tap(&mut h, pos2, 3);
    assert_eq!(curves(&h), 1, "Novo segmento deve estar apenas em staging");
    assert!(h.app.tool.as_ref().unwrap().awaiting_confirmation);

    // Cancelar com ✕ via touch_tap
    let cancel_center = crate::tools::touch_confirm_rects(&h.app)[1].center();
    touch_tap(&mut h, cancel_center, 11);

    // ✕ descartou apenas o pendente; segmento A -> B permanece!
    assert_eq!(curves(&h), 1, "✕ não pode apagar segmentos já confirmados");
    let tool = h.app.tool.as_ref().unwrap();
    assert_eq!(tool.pts.len(), 1, "Cadeia continua pronta a partir de B");
    assert_eq!(tool.pts[0].0, [20.0, 15.0].into());
    assert!(!tool.awaiting_confirmation);
}

/// 3. line_drag_draws_one com toque: soltar deixa em staging.
#[test]
fn line_drag_draws_one_touch_drag_stages_preview() {
    let mut h = harness("sketch.line");
    h.app.preferences.line_drag_draws_one = true;
    let pos0 = screen_pos(&mut h, [0.0, 0.0]);
    let pos1 = screen_pos(&mut h, [25.0, 20.0]);
    assert!(!crate::shell::covered(&h.app, pos0));
    assert!(!crate::shell::covered(&h.app, pos1));

    let dev = egui::TouchDeviceId(0);
    let tid = egui::TouchId(20);

    // Pousar toque em pos0
    h.frame_with(vec![
        egui::Event::Touch {
            device_id: dev,
            id: tid,
            phase: egui::TouchPhase::Start,
            pos: pos0,
            force: None,
        },
        egui::Event::PointerMoved(pos0),
        egui::Event::PointerButton {
            pos: pos0,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    ]);

    // Arrastar até pos1 (> 24 pt)
    h.frame_with(vec![
        egui::Event::Touch {
            device_id: dev,
            id: tid,
            phase: egui::TouchPhase::Move,
            pos: pos1,
            force: None,
        },
        egui::Event::PointerMoved(pos1),
    ]);

    // Soltar toque em pos1
    h.frame_with(vec![
        egui::Event::Touch {
            device_id: dev,
            id: tid,
            phase: egui::TouchPhase::End,
            pos: pos1,
            force: None,
        },
        egui::Event::PointerButton {
            pos: pos1,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::PointerGone,
    ]);
    h.settle();

    assert_eq!(curves(&h), 0, "Soltar arrasto de linha com toque não deve criar segmento imediatamente");
    assert!(h.app.tool.as_ref().unwrap().awaiting_confirmation, "Deve ficar em staging aguardando confirmação");
}

/// 4. Cadeia com 2 segmentos confirmados + 1 pendente, pinça escalonada: tudo continua.
#[test]
fn line_chain_staggered_pinch_preserves_confirmed_and_staged_state() {
    let mut h = harness("sketch.line");
    let pos0 = screen_pos(&mut h, [0.0, 0.0]);
    let pos1 = screen_pos(&mut h, [20.0, 0.0]);
    let pos2 = screen_pos(&mut h, [20.0, 20.0]);
    let pos3 = screen_pos(&mut h, [10.0, 30.0]);
    assert!(!crate::shell::covered(&h.app, pos0));
    assert!(!crate::shell::covered(&h.app, pos1));
    assert!(!crate::shell::covered(&h.app, pos2));
    assert!(!crate::shell::covered(&h.app, pos3));

    // Confirmar 1º segmento (pos0 -> pos1)
    touch_tap(&mut h, pos0, 1);
    touch_tap(&mut h, pos1, 2);
    assert_eq!(curves(&h), 0, "Dois toques devem deixar o 1º segmento em staging");
    assert!(h.app.tool.as_ref().unwrap().awaiting_confirmation, "Deve estar em staging");
    let c1 = crate::tools::touch_confirm_rects(&h.app)[0].center();
    touch_tap(&mut h, c1, 3);
    assert_eq!(curves(&h), 1);

    // Confirmar 2º segmento (pos1 -> pos2)
    touch_tap(&mut h, pos2, 4);
    let c2 = crate::tools::touch_confirm_rects(&h.app)[0].center();
    touch_tap(&mut h, c2, 5);
    assert_eq!(curves(&h), 2);

    // Colocar 3º segmento em staging (pos2 -> pos3)
    touch_tap(&mut h, pos3, 6);
    assert_eq!(curves(&h), 2);
    assert!(h.app.tool.as_ref().unwrap().awaiting_confirmation);

    // Pinça escalonada: Dedo A e Dedo B pousam sem deslocamento significativo (tap de dois dedos / soltura limpa)
    let dev = egui::TouchDeviceId(0);
    let finger_a1 = screen_pos(&mut h, [-15.0, -10.0]);
    let finger_b1 = screen_pos(&mut h, [-25.0, -20.0]);
    assert!(!crate::shell::covered(&h.app, finger_a1));
    assert!(!crate::shell::covered(&h.app, finger_b1));

    // Frame 1: Dedo A pousa
    h.frame_with(vec![
        egui::Event::Touch {
            device_id: dev,
            id: egui::TouchId(40),
            phase: egui::TouchPhase::Start,
            pos: finger_a1,
            force: None,
        },
        egui::Event::PointerMoved(finger_a1),
        egui::Event::PointerButton {
            pos: finger_a1,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    ]);

    // Frame 2: Dedo B pousa
    h.frame_with(vec![
        egui::Event::Touch {
            device_id: dev,
            id: egui::TouchId(41),
            phase: egui::TouchPhase::Start,
            pos: finger_b1,
            force: None,
        },
    ]);

    // Frame 3: Ambos os dedos soltam sem arrasto
    h.frame_with(vec![
        egui::Event::Touch {
            device_id: dev,
            id: egui::TouchId(40),
            phase: egui::TouchPhase::End,
            pos: finger_a1,
            force: None,
        },
        egui::Event::Touch {
            device_id: dev,
            id: egui::TouchId(41),
            phase: egui::TouchPhase::End,
            pos: finger_b1,
            force: None,
        },
        egui::Event::PointerButton {
            pos: finger_a1,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::PointerGone,
    ]);
    h.settle();

    // 2 curvas continuam, o pendente pos3 continua em staging sem criar lixo
    assert_eq!(curves(&h), 2, "Pinça não deve comitar curvas nem estragar as existentes");
    let tool = h.app.tool.as_ref().unwrap();
    assert!(tool.awaiting_confirmation, "Staging do 3º segmento deve ser preservado pela pinça");
    assert_eq!(tool.confirmed_segments, 2, "confirmed_segments deve ser preservado pelo snapshot");
    assert_eq!(tool.pts.last().map(|p| p.0), Some([10.0, 30.0].into()), "Ponto em staging deve ser preservado intacto pelo snapshot");
    assert!(crate::tools::conclude_rect(&h.app).is_some(), "Botão Concluir deve continuar visível após a pinça");
}

/// 5. Desfazer remove um segmento por vez.
#[test]
fn line_chain_undo_removes_one_segment_at_a_time() {
    let mut h = harness("sketch.line");
    let pos0 = screen_pos(&mut h, [0.0, 0.0]);
    let pos1 = screen_pos(&mut h, [20.0, 0.0]);
    let pos2 = screen_pos(&mut h, [20.0, 20.0]);
    assert!(!crate::shell::covered(&h.app, pos0));
    assert!(!crate::shell::covered(&h.app, pos1));
    assert!(!crate::shell::covered(&h.app, pos2));

    // Confirmar 1º segmento
    touch_tap(&mut h, pos0, 1);
    touch_tap(&mut h, pos1, 2);
    assert!(h.app.tool.as_ref().unwrap().awaiting_confirmation, "1º segmento deve aguardar confirmação");
    let c1 = crate::tools::touch_confirm_rects(&h.app)[0].center();
    touch_tap(&mut h, c1, 3);
    assert_eq!(curves(&h), 1);

    // Confirmar 2º segmento
    touch_tap(&mut h, pos2, 4);
    let c2 = crate::tools::touch_confirm_rects(&h.app)[0].center();
    touch_tap(&mut h, c2, 5);
    assert_eq!(curves(&h), 2);

    // Desfazer (edit.undo) uma vez: deve restar exatamente 1 segmento
    h.app.run("edit.undo", json!({})).unwrap();
    h.settle();
    assert_eq!(curves(&h), 1, "Desfazer deve remover apenas o 2º segmento");

    // Desfazer novamente: deve restar 0 segmentos
    h.app.run("edit.undo", json!({})).unwrap();
    h.settle();
    assert_eq!(curves(&h), 0, "Segundo desfazer deve remover o 1º segmento");
}

/// 6a. Botão Concluir encerra a cadeia.
#[test]
fn line_chain_conclude_button_ends_chain() {
    let mut h = harness("sketch.line");
    let pos0 = screen_pos(&mut h, [0.0, 0.0]);
    let pos1 = screen_pos(&mut h, [20.0, 0.0]);
    assert!(!crate::shell::covered(&h.app, pos0));
    assert!(!crate::shell::covered(&h.app, pos1));

    // Colocar e confirmar 1 segmento
    touch_tap(&mut h, pos0, 1);
    touch_tap(&mut h, pos1, 2);
    assert!(h.app.tool.as_ref().unwrap().awaiting_confirmation, "1º segmento deve aguardar confirmação");
    let c1 = crate::tools::touch_confirm_rects(&h.app)[0].center();
    touch_tap(&mut h, c1, 3);
    assert_eq!(curves(&h), 1);

    // O botão "Concluir" deve existir enquanto houver cadeia ativa
    let conclude = crate::tools::conclude_rect(&h.app);
    assert!(conclude.is_some(), "Botão Concluir deve estar presente na UI durante cadeia ativa");
    let conclude_rect = conclude.unwrap();
    let conclude_center = conclude_rect.center();

    // Validar que não fica coberto por outros painéis (excluindo os botões de confirmação e concluir)
    let touch_rects = crate::tools::touch_confirm_rects(&h.app);
    let panel_covered = crate::shell::covered_rects(&h.app)
        .into_iter()
        .filter(|r| *r != conclude_rect && !touch_rects.iter().any(|br| br == r))
        .any(|r| r.contains(conclude_center));
    assert!(!panel_covered, "Botão Concluir não pode ficar sob painéis");

    // Validar que não sobrepõe ✓ / ✕ se houver
    for tr in &touch_rects {
        assert!(!conclude_rect.intersects(*tr), "Botão Concluir não deve sobrepor ✓/✕");
    }

    touch_tap(&mut h, conclude_center, 4);
    assert_eq!(curves(&h), 1, "Segmento confirmado permanece");
    assert!(h.app.tool.as_ref().unwrap().pts.is_empty(), "Cadeia deve ter sido encerrada");
}

/// 6a2. Botão Concluir com segmento pendente em staging: confirma o pendente e encerra a cadeia.
#[test]
fn line_chain_conclude_with_staged_segment_confirms_and_ends_chain() {
    let mut h = harness("sketch.line");
    let pos0 = screen_pos(&mut h, [0.0, 0.0]);
    let pos1 = screen_pos(&mut h, [20.0, 0.0]);
    let pos2 = screen_pos(&mut h, [20.0, 20.0]);
    assert!(!crate::shell::covered(&h.app, pos0));
    assert!(!crate::shell::covered(&h.app, pos1));
    assert!(!crate::shell::covered(&h.app, pos2));

    // Confirmar 1º segmento
    touch_tap(&mut h, pos0, 1);
    touch_tap(&mut h, pos1, 2);
    let c1 = crate::tools::touch_confirm_rects(&h.app)[0].center();
    touch_tap(&mut h, c1, 3);
    assert_eq!(curves(&h), 1);

    // 2º segmento em staging
    touch_tap(&mut h, pos2, 4);
    assert_eq!(curves(&h), 1);
    assert!(h.app.tool.as_ref().unwrap().awaiting_confirmation);

    // Concluir no rodapé com o 2º segmento em staging
    let conclude = crate::tools::conclude_rect(&h.app).expect("Concluir visível com cadeia ativa");
    touch_tap(&mut h, conclude.center(), 5);

    // O 2º segmento deve ter sido confirmado (2 curvas) e a ferramenta encerrada
    assert_eq!(curves(&h), 2, "Concluir com staging deve confirmar o segmento pendente");
    assert!(h.app.tool.as_ref().unwrap().pts.is_empty(), "Cadeia deve ter sido encerrada");
}

/// 6b. Tecla Esc sem segmento pendente encerra a cadeia.
#[test]
fn line_chain_escape_without_pending_ends_chain() {
    let mut h = harness("sketch.line");
    let pos0 = screen_pos(&mut h, [0.0, 0.0]);
    let pos1 = screen_pos(&mut h, [20.0, 0.0]);
    assert!(!crate::shell::covered(&h.app, pos0));
    assert!(!crate::shell::covered(&h.app, pos1));

    // Colocar e confirmar 1 segmento
    touch_tap(&mut h, pos0, 1);
    touch_tap(&mut h, pos1, 2);
    assert!(h.app.tool.as_ref().unwrap().awaiting_confirmation, "1º segmento deve aguardar confirmação");
    let c1 = crate::tools::touch_confirm_rects(&h.app)[0].center();
    touch_tap(&mut h, c1, 3);
    assert_eq!(curves(&h), 1);
    assert!(!h.app.tool.as_ref().unwrap().awaiting_confirmation);

    // Pressionar Esc
    h.frame_with(vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    h.settle();

    assert_eq!(curves(&h), 1);
    assert!(h.app.tool.as_ref().unwrap().pts.is_empty(), "Esc sem segmento pendente encerra a cadeia");
}

/// 6c. Fechar no ponto inicial: tocar no ponto inicial coloca o último ponto lá em staging;
/// ✓ confirma o segmento, une os pontos com coincidência e encerra a cadeia fechada,
/// gerando um perfil extrudável.
#[test]
fn line_chain_close_at_origin_creates_extrudable_profile() {
    let mut h = harness("sketch.line");
    let pos0 = screen_pos(&mut h, [0.0, 0.0]);
    let pos1 = screen_pos(&mut h, [20.0, 0.0]);
    let pos2 = screen_pos(&mut h, [20.0, 20.0]);
    let pos0_close = screen_pos(&mut h, [0.0, 0.0]); // snap no ponto inicial
    assert!(!crate::shell::covered(&h.app, pos0));
    assert!(!crate::shell::covered(&h.app, pos1));
    assert!(!crate::shell::covered(&h.app, pos2));
    assert!(!crate::shell::covered(&h.app, pos0_close));

    // A -> B
    touch_tap(&mut h, pos0, 1);
    touch_tap(&mut h, pos1, 2);
    assert!(h.app.tool.as_ref().unwrap().awaiting_confirmation, "1º segmento deve aguardar confirmação");
    let c1 = crate::tools::touch_confirm_rects(&h.app)[0].center();
    touch_tap(&mut h, c1, 3);

    // B -> C
    touch_tap(&mut h, pos2, 4);
    let c2 = crate::tools::touch_confirm_rects(&h.app)[0].center();
    touch_tap(&mut h, c2, 5);

    // C -> A (fecho na origem da cadeia)
    touch_tap(&mut h, pos0_close, 6);
    assert_eq!(curves(&h), 2, "Fecho na origem deve primeiro ficar em staging");
    assert!(h.app.tool.as_ref().unwrap().awaiting_confirmation);

    // Confirmar com ✓
    let c3 = crate::tools::touch_confirm_rects(&h.app)[0].center();
    touch_tap(&mut h, c3, 7);

    assert_eq!(curves(&h), 3, "3 segmentos criados formando triângulo");
    assert!(h.app.tool.as_ref().unwrap().pts.is_empty(), "Cadeia fecha e encerra automaticamente após fecho na origem");

    // Verificar se o contorno fechado gerou perfil extrudável
    h.app.run("sketch.finish", json!({})).unwrap();
    let ext_res = h.app.run("solid.extrude", json!({"distance": 10.0}));
    assert!(ext_res.is_ok(), "Contorno fechado deve formar um perfil extrudável com sucesso");
}

/// 7. Tecla Enter confirma o segmento pendente e a cadeia continua ativa.
#[test]
fn line_chain_enter_key_confirms_segment_and_continues_chain() {
    let mut h = harness("sketch.line");
    let pos0 = screen_pos(&mut h, [0.0, 0.0]);
    let pos1 = screen_pos(&mut h, [20.0, 10.0]);
    assert!(!crate::shell::covered(&h.app, pos0));
    assert!(!crate::shell::covered(&h.app, pos1));

    touch_tap(&mut h, pos0, 1);
    touch_tap(&mut h, pos1, 2);
    assert!(h.app.tool.as_ref().unwrap().awaiting_confirmation);

    // Pressionar Enter
    h.frame_with(vec![egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    h.settle();

    assert_eq!(curves(&h), 1, "Enter deve confirmar o segmento");
    let tool = h.app.tool.as_ref().unwrap();
    assert_eq!(tool.pts.len(), 1, "Cadeia deve continuar com o ponto final");
    assert_eq!(tool.pts[0].0, [20.0, 10.0].into());
    assert!(!tool.awaiting_confirmation);
}

/// 7b. Tecla Enter confirma o segmento na linha COM caixas de dimensão presentes.
#[test]
fn line_chain_enter_key_confirms_segment_with_dimension_boxes() {
    let mut h = harness("sketch.line");
    let pos0 = screen_pos(&mut h, [0.0, 0.0]);
    let pos1 = screen_pos(&mut h, [20.0, 0.0]);
    assert!(!crate::shell::covered(&h.app, pos0));
    assert!(!crate::shell::covered(&h.app, pos1));

    touch_tap(&mut h, pos0, 1);
    touch_tap(&mut h, pos1, 2);
    assert!(h.app.tool.as_ref().unwrap().awaiting_confirmation);
    // As caixas de dimensão da linha são criadas a partir do 1º ponto
    assert!(!h.app.tool.as_ref().unwrap().dims.is_empty(), "Ferramenta deve possuir caixas de dimensão");

    // Pressionar Enter sem foco em caixa: deve confirmar o segmento via shortcut global
    h.frame_with(vec![egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    h.settle();

    assert_eq!(curves(&h), 1, "Enter sem foco em caixa deve confirmar o segmento na linha com dimensões");
    let tool = h.app.tool.as_ref().unwrap();
    assert_eq!(tool.pts.len(), 1);
    assert!(!tool.awaiting_confirmation);
}

/// 7c. Tecla Enter confirma retângulo em staging COM caixas de dimensão presentes.
#[test]
fn rectangle_enter_key_confirms_staged_with_dimension_boxes() {
    let mut h = harness("sketch.rectangle.two_point");
    let pos0 = screen_pos(&mut h, [0.0, 0.0]);
    let pos1 = screen_pos(&mut h, [30.0, 20.0]);

    touch_tap(&mut h, pos0, 1);
    touch_tap(&mut h, pos1, 2);
    assert!(h.app.tool.as_ref().unwrap().awaiting_confirmation);
    assert!(!h.app.tool.as_ref().unwrap().dims.is_empty(), "Retângulo deve possuir caixas de dimensão em staging");

    h.frame_with(vec![egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    h.settle();

    assert_eq!(curves(&h), 4, "Enter sem foco deve confirmar o retângulo com caixas de dimensão");
    assert!(h.app.tool.as_ref().unwrap().pts.is_empty());
}

/// 8. Hover da caneta nunca coloca ponto na linha encadeada.
#[test]
fn line_chain_pen_hover_creates_no_points_or_segments() {
    let mut h = harness("sketch.line");
    let pos0 = screen_pos(&mut h, [0.0, 0.0]);
    let pos1 = screen_pos(&mut h, [25.0, 15.0]);
    assert!(!crate::shell::covered(&h.app, pos0));
    assert!(!crate::shell::covered(&h.app, pos1));

    // Caneta pairando sobre pos0 (sem tocar)
    h.call("ui.move", json!({"x": pos0.x, "y": pos0.y}));
    assert_eq!(curves(&h), 0);
    assert!(h.app.tool.as_ref().unwrap().pts.is_empty(), "Hover antes do 1º ponto não coloca ponto");

    // Tocar ponto 0
    touch_tap(&mut h, pos0, 1);
    assert_eq!(h.app.tool.as_ref().unwrap().pts.len(), 1);

    // Caneta pairando até pos1 (sem tocar)
    h.call("ui.move", json!({"x": pos1.x, "y": pos1.y}));
    assert_eq!(curves(&h), 0, "Hover após o 1º ponto não cria segmento");
    assert_eq!(h.app.tool.as_ref().unwrap().pts.len(), 1, "Hover não adiciona novos pontos à cadeia");
}

