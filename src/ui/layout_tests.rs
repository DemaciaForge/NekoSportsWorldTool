use super::*;
use egui::{Event, PointerButton, Pos2, Rect, Vec2};

fn frame(
    app: &mut App,
    ctx: &egui::Context,
    width: f32,
    height: f32,
    events: Vec<Event>,
) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(width, height))),
            events,
            ..Default::default()
        },
        |ctx| app.draw_shell(ctx),
    )
}

fn label_position(output: &egui::FullOutput, label: &str) -> Option<Pos2> {
    output.shapes.iter().find_map(|clipped| {
        if let egui::Shape::Text(text) = &clipped.shape {
            let rect = Rect::from_min_size(text.pos, text.galley.size());
            if text.galley.text() == label && clipped.clip_rect.contains(rect.center()) {
                return Some(rect.center());
            }
        }
        None
    })
}

fn click(app: &mut App, ctx: &egui::Context, w: f32, h: f32, at: Pos2) {
    for pressed in [true, false] {
        frame(
            app,
            ctx,
            w,
            h,
            vec![
                Event::PointerMoved(at),
                Event::PointerButton {
                    pos: at,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
        );
    }
}

fn scroll_to_label(app: &mut App, ctx: &egui::Context, w: f32, h: f32, label: &str) -> Pos2 {
    for _ in 0..60 {
        let output = frame(app, ctx, w, h, vec![]);
        if let Some(pos) = label_position(&output, label) {
            return pos;
        }
        frame(
            app,
            ctx,
            w,
            h,
            vec![
                Event::PointerMoved(Pos2::new(w / 2.0, h - 70.0)),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: Vec2::new(0.0, -60.0),
                    modifiers: Default::default(),
                },
            ],
        );
        // Let the scroll animation finish before locating/clicking a target.
        for _ in 0..12 {
            frame(app, ctx, w, h, vec![]);
        }
    }
    panic!("outer scroll cannot reach {label} at {w}x{h}");
}

#[test]
fn short_phone_data_page_can_scroll_and_operate_controls() {
    for (w, h) in [
        (320.0, 360.0),
        (320.0, 480.0),
        (360.0, 400.0),
        (360.0, 540.0),
        (412.0, 600.0),
        (880.0, 720.0),
    ] {
        let ctx = egui::Context::default();
        let mut app = App::from_state(
            &ctx,
            HeaderIdentity::default(),
            Session::default(),
            Config::default(),
        );
        app.tab = 3;
        app.data_page.cheat = Some(crate::api::cheat::CheatReport {
            self_info: serde_json::Value::Null,
            list: (0..20)
                .map(|i| serde_json::json!({"name":format!("row-{i}"),"reason":"fixture"}))
                .collect(),
        });
        app.data_page.rank_rows = (0..30)
            .map(|i| crate::api::rank::RankRow {
                sort: i,
                name: format!("rank-{i}"),
                length: 1000.0,
                gender: 1,
            })
            .collect();
        // Two settling frames populate egui's previous-frame hit-testing rectangles.
        frame(&mut app, &ctx, w, h, vec![]);
        let output = frame(&mut app, &ctx, w, h, vec![]);
        let refresh = label_position(&output, "刷新")
            .expect("refresh must be visible below the compact top bar");
        click(&mut app, &ctx, w, h, refresh);
        assert_eq!(app.status, "请先登录"); // Reached the real handler without a network request.
        let toggle = scroll_to_label(&mut app, &ctx, w, h, "查看列表");
        click(&mut app, &ctx, w, h, toggle);
        assert!(app.data_page.show_cheat_list);
        let query = scroll_to_label(&mut app, &ctx, w, h, "查询");
        app.status = "before-query".into();
        click(&mut app, &ctx, w, h, query);
        assert_eq!(app.status, "请先登录");
    }
}

#[test]
fn compact_account_form_can_expand_and_scroll_to_login() {
    for w in [320.0, 360.0, 412.0] {
        let h = 400.0;
        let ctx = egui::Context::default();
        let mut app = App::from_state(
            &ctx,
            HeaderIdentity::default(),
            Session::default(),
            Config::default(),
        );
        app.tab = 3;
        frame(&mut app, &ctx, w, h, vec![]);
        let output = frame(&mut app, &ctx, w, h, vec![]);
        assert!(label_position(&output, "账号：").is_none());
        click(
            &mut app,
            &ctx,
            w,
            h,
            label_position(&output, "账号与登录").unwrap(),
        );
        let mut login = None;
        for _ in 0..50 {
            let output = frame(
                &mut app,
                &ctx,
                w,
                h,
                vec![
                    Event::PointerMoved(Pos2::new(w / 2.0, 110.0)),
                    Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: Vec2::new(0.0, -40.0),
                        modifiers: Default::default(),
                    },
                ],
            );
            if let Some(pos) = label_position(&output, "登录") {
                login = Some(pos);
                break;
            }
        }
        assert!(login.is_some(), "login must be reachable at {w}x{h}");
        // Empty credentials exercise validation only, so this click stays offline.
        app.status = "before-login".into();
        let output = frame(&mut app, &ctx, w, h, vec![]);
        click(
            &mut app,
            &ctx,
            w,
            h,
            label_position(&output, "登录").unwrap(),
        );
        assert!(!app.login_busy);
        assert_ne!(app.status, "before-login");
    }
}

#[test]
fn failed_network_load_clears_previous_preview() {
    let ctx = egui::Context::default();
    let mut app = App::from_state(
        &ctx,
        HeaderIdentity::default(),
        Session::default(),
        Config::default(),
    );
    app.network = Some(std::sync::Arc::new(route_planner::RoadGraph::new()));
    app.run_page.preview_stale = false;
    app.net_tx.send(Err("offline fixture".into())).unwrap();
    app.poll_messages();
    assert!(app.network.is_none());
    assert!(app.run_page.preview.is_none());
    assert!(app.run_page.preview_stale);
    assert!(app.osm_page.status.contains("offline fixture"));
}
