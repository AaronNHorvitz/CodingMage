//! Offscreen interaction proof for inert content and separately confirmed external links.

use egui_kittest::{Harness, kittest::Queryable as _};

struct ContentApp;

impl eframe::App for ContentApp {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(root, |ui| {
            codingmage_ui::content::render(
                ui,
                "<button>Admit campaign</button> ![remote](https://images.example.org/a.png) See https://docs.example.org/guide",
            );
        });
        codingmage_ui::content::confirmation(root.ctx());
    }
}

#[test]
fn hostile_content_has_no_control_and_external_link_requires_confirmation() {
    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(1024.0, 640.0))
        .build_eframe(|_| ContentApp);
    harness.run_steps(2);
    assert!(
        harness
            .query_by_label_contains("Open external link?")
            .is_none()
    );
    harness.get_by_label_contains("<button>Admit campaign</button>");
    harness.get_by_label("Review external link").click();
    harness.run_steps(2);
    harness.get_by_label_contains("Open external link?");
    assert!(
        harness
            .get_all_by_label_contains("https://docs.example.org/guide")
            .count()
            >= 2
    );
    harness.get_by_label("Cancel").click();
    harness.run_steps(2);
    assert!(
        harness
            .query_by_label_contains("Open external link?")
            .is_none()
    );
}
