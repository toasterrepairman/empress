use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{cairo, glib, graphene};
use std::cell::Cell;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct ProgressRingButton {
        pub progress: Cell<f64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ProgressRingButton {
        const NAME: &'static str = "ProgressRingButton";
        type Type = super::ProgressRingButton;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_layout_manager_type::<gtk::BinLayout>();
            klass.set_css_name("progressringbutton");
        }
    }

    impl ObjectImpl for ProgressRingButton {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            // Create the button child
            let button = gtk::Button::builder()
                .icon_name("media-playback-start-symbolic")
                .css_classes(vec!["circular", "suggested-action"])
                .width_request(48)
                .height_request(48)
                .build();

            button.set_parent(&*obj);
        }

        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for ProgressRingButton {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let widget = self.obj();
            let width = widget.width() as f32;
            let height = widget.height() as f32;
            let progress = self.progress.get() as f32;

            // Draw the child button first
            self.parent_snapshot(snapshot);

            if progress > 0.0 {
                let center_x = width / 2.0;
                let center_y = height / 2.0;
                let radius = (width.min(height) / 2.0) - 4.0; // Leave some margin
                let line_width = 3.0;

                // Create a cairo context
                let rect = graphene::Rect::new(0.0, 0.0, width, height);
                let cr = snapshot.append_cairo(&rect);

                // Get the theme color using the new API
                let style_context = widget.style_context();
                let color = style_context.color();

                // Set up cairo for the progress ring
                cr.set_source_rgba(
                    color.red() as f64,
                    color.green() as f64,
                    color.blue() as f64,
                    0.8,
                );
                cr.set_line_width(line_width as f64);
                cr.set_line_cap(cairo::LineCap::Round);

                // Draw the progress arc
                // Start at -90 degrees (top) and go clockwise
                let start_angle = -std::f64::consts::FRAC_PI_2;
                let end_angle = start_angle + (2.0 * std::f64::consts::PI * progress as f64);

                cr.arc(
                    center_x as f64,
                    center_y as f64,
                    radius as f64,
                    start_angle,
                    end_angle,
                );
                cr.stroke().ok();
            }
        }
    }
}

glib::wrapper! {
    pub struct ProgressRingButton(ObjectSubclass<imp::ProgressRingButton>)
        @extends gtk::Widget;
}

impl ProgressRingButton {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_progress(&self, progress: f64) {
        let progress = progress.clamp(0.0, 1.0);
        self.imp().progress.set(progress);
        self.queue_draw();
    }

    pub fn button(&self) -> gtk::Button {
        self.first_child()
            .and_downcast::<gtk::Button>()
            .expect("First child should be a button")
    }

    pub fn set_icon_name(&self, icon_name: &str) {
        self.button().set_icon_name(icon_name);
    }

    pub fn set_paused_style(&self, is_paused: bool) {
        let button = self.button();
        if is_paused {
            // When paused (showing play icon), use suggested-action (blue)
            button.remove_css_class("accent");
            button.add_css_class("suggested-action");
        } else {
            // When playing (showing pause icon), use accent color
            button.remove_css_class("suggested-action");
            button.add_css_class("accent");
        }
    }
}

impl Default for ProgressRingButton {
    fn default() -> Self {
        Self::new()
    }
}
