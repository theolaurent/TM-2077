#![warn(clippy::all)]
// Strict no-panic policy (see AGENTS.md): production code must not use
// `unwrap`/`expect`/`panic!`/`unreachable!`/`todo!`/`unimplemented!`. These
// restriction lints enforce it. Test modules (`#[cfg(test)]`) are exempt and
// not compiled here.
#![warn(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::unreachable
)]

mod app;
mod audio;
mod note;
mod theme;
mod ui;

pub use app::Tm2077App;

// ---------------------------------------------------------------------------
// Native entry point (desktop: Linux / macOS / Windows)
// ---------------------------------------------------------------------------
#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    env_logger::init();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([860.0, 560.0])
            .with_min_inner_size([640.0, 420.0])
            .with_title("TM-2077 · Tuner / Metronome"),
        ..Default::default()
    };

    eframe::run_native(
        "TM-2077",
        native_options,
        Box::new(|cc| Ok(Box::new(Tm2077App::new(cc)))),
    )
}

// ---------------------------------------------------------------------------
// Web entry point (wasm via `trunk serve`)
// ---------------------------------------------------------------------------
#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    // Show panics in the browser console instead of a cryptic "unreachable".
    console_error_panic_hook::set_once();
    eframe::WebLogger::init(log::LevelFilter::Debug).ok();

    let web_options = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async {
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            log::error!("boot: no window/document available");
            return;
        };

        let Some(canvas) = document
            .get_element_by_id("the_canvas_id")
            .and_then(|el| el.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        else {
            log::error!("boot: `the_canvas_id` <canvas> not found");
            return;
        };

        let start_result = eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|cc| Ok(Box::new(Tm2077App::new(cc)))),
            )
            .await;

        // Drop the "booting…" placeholder once egui takes over.
        if let Some(loading) = document.get_element_by_id("loading") {
            match start_result {
                Ok(_) => loading.remove(),
                Err(e) => {
                    loading.set_inner_html(
                        "<p>Failed to start TM-2077. See the developer console for details.</p>",
                    );
                    log::error!("boot: failed to start eframe: {e:?}");
                }
            }
        }
    });
}
