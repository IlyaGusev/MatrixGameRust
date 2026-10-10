use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowAttributes, WindowId},
};

use crate::gfx::context::GfxContext;
use crate::matrix_game::camera::Camera;
use crate::matrix_game::effects::point_light::PointLightSystem;
use crate::matrix_game::logic::MapLogic;
use crate::matrix_game::map::GameMap;
use crate::matrix_game::map::MapRenderer;
use crate::matrix_game::minimap::Minimap;
struct AppState {
    window: Arc<Window>,
    gfx: GfxContext,
    map: Arc<GameMap>,
    point_lights: PointLightSystem,
    /// Live follow-light handles keyed by owning weapon (plasma/flame glow).
    light_follow_map: std::collections::HashMap<
        crate::matrix_game::effects::weapon::WeaponId,
        crate::matrix_game::effects::point_light::PointLightId,
    >,
    terrain: MapRenderer,
    minimap: Minimap,
    camera: Camera,
    game: MapLogic,
    last_time: f64,
    cursor: [f32; 2],
    /// Software cursor — port of `CMatrixMap::m_Cursor`
    /// (CMatrixCursor). Mode selection happens in `update_cursor`,
    /// the sprite draws in the final overlay pass.
    matrix_cursor: crate::matrix_game::cursor::MatrixCursor,
    /// `m_TraceStopObj` / `m_TraceStopPos` — the per-frame mouse
    /// ray-trace result (MatrixMap.cpp:1149-1178). Frozen while
    /// paused, like the C++ (the trace lives in `CMatrixMap::Takt`).
    mouse_trace: crate::matrix_game::map_trace::TraceStop,
    mouse_trace_pos: glam::Vec3,
    minimap_dragging: bool,
    /// Shift-key modifier state — tracked so left-click can toggle
    /// the multi-selection (`CMultiSelection::Add/Remove` at
    /// MatrixSide.cpp:1584-1598) vs. replace it.
    shift_down: bool,
    /// Left-button press anchor (screen coords) while the button is
    /// held. `Some` → either a pending click or an in-progress
    /// marquee-drag; `None` → button released. `CMultiSelection::Begin`
    /// in the C++ stores the same state (MatrixMultiSelection.cpp).
    lmb_anchor: Option<[f32; 2]>,
    /// Previous click (time_ms, x, y) for double-click detection —
    /// drives `OnLButtonDouble` (MatrixFormGame.cpp WM_LBUTTONDBLCLK).
    last_click: Option<(i64, f32, f32)>,
    /// Whether the current LMB press was consumed by UI / minimap
    /// (if so, release mustn't issue a world click or marquee).
    lmb_consumed_by_ui: bool,
    /// Last rect the marquee rendered / was releasing. Needed across
    /// frames because the DIP fade keeps drawing for 50ms after the
    /// user lifts LMB and has to resample alpha off the same coords.
    marquee_last_rect: Option<[f32; 4]>,
    /// Selection-ring renderer — ports `CMatrixEffectSelection`
    /// (MatrixEffectSelection.cpp). Drawn over the terrain after the
    /// object pass; green ring on the ground around the selected
    /// object.
    selection_ring: crate::matrix_game::effects::selection::SelectionRingRenderer,
    /// Screen-space marquee rectangle drawn while the user holds
    /// left-button and drags on empty terrain. Rebuilt each frame
    /// from `lmb_anchor` + current cursor; hidden otherwise.
    marquee: crate::matrix_game::multi_selection::MarqueeRenderer,
    /// Move-order ground-ping effect — port of `CMatrixEffectMoveto`
    /// (Effects/MatrixEffectMoveTo.cpp). Spawned at the terrain hit
    /// point when a right-click issues a move order while robots are
    /// selected; lifetime 400ms.
    move_to: crate::matrix_game::effects::move_to::MoveToRenderer,
    /// Loaded UI panels (IF_MAIN / IF_BASE / etc.) + focus state.
    /// Ported from `CIFaceList` (Interface/CInterface.h:269+).
    iface_list: crate::matrix_game::interface::IFaceList,
    /// 2D textured-quad renderer for the UI panels. Drawn last so
    /// the HUD sits on top of world + minimap.
    iface_renderer: crate::matrix_game::interface::InterfaceRenderer,
    /// Horizontal 3-segment progress bars — port of
    /// `CMatrixProgressBar`. Queued each frame from
    /// `refresh_progress_bars`; drawn after the interface pass.
    progress_bars: crate::matrix_game::progress_bar::ProgressBarRenderer,
    /// Robot-constructor 3D preview state — port of
    /// `CConstructor::Render`'s viewport setup (CConstructor.cpp:
    /// 264-360). Emits a chassis draw-ticket per frame while the
    /// constructor panel is active.
    builder_preview: crate::matrix_game::interface::constructor::BuilderPreview,
    /// Cached robot-icon textures for the build-queue UI. One 64×64
    /// per unique `RobotConfig`, lazily baked the first time the queue
    /// surfaces a config. Port of `m_MedTexture` (MatrixRobot.cpp:
    /// 5342-5380); see `interface::robot_icons` for the bake path.
    robot_icons: crate::matrix_game::interface::RobotIconCache,
    /// Fullscreen translucent-black overlay drawn when `is_paused` —
    /// port of MatrixMap.cpp:2430-2454.
    pause_overlay: crate::matrix_game::pause_overlay::PauseOverlay,
    /// Mirrors `g_MatrixMap->IsPaused()` (MatrixMap.hpp:640). Constructor
    /// open / close toggles this; logic takt is gated on it.
    is_paused: bool,
    /// Dialog-mode state — port of `m_DialogModeName` +
    /// `m_DialogModeHints` + MMFLAG_DIALOG_MODE (MatrixMap.cpp:
    /// 3261-3574). While `Some`, the game is paused and mouse/keys
    /// route to the hint buttons only.
    dialog: Option<crate::matrix_game::interface::dialog::DialogState>,
    /// `g_ExitState` — why the session ended (1 exit, 2 lose, 3 win,
    /// 4 surrender). Logged on exit; the SR2 host consumed it.
    exit_state: i32,
    /// Keys whose press was consumed by `handle_game_key` — their
    /// auto-repeats must not leak into the camera bindings (held `S`
    /// would both issue a stop order and scroll the camera otherwise).
    consumed_keys: std::collections::HashSet<winit::keyboard::KeyCode>,
    /// Recently typed letters — the `m_LastScans` ring the C++ matches
    /// cheat words against (MatrixFormGame.cpp:888-947). Only the
    /// FLYCAM word is ported.
    cheat_keys: String,
    /// `m_AFD` — live while MMFLAG_FLYCAM (`objects.fly_cam`) is set.
    fly_cam: Option<crate::matrix_game::camera::AutoFlyData>,
    /// Physical movement keys currently held — the port of the
    /// `GetAsyncKeyState(KA_UNIT_*)` polls the arcade branches read
    /// (WASD + arrows; see `sync_arcade_input`).
    held_keys: std::collections::HashSet<winit::keyboard::KeyCode>,
    /// Left mouse button held (`GetAsyncKeyState(KA_FIRE)` port).
    lmb_down: bool,
    /// The `if/Main` panel currently sits at the arcade x-offset
    /// (+196px, CInterface.cpp:3371/3392) — reconciled each frame
    /// against `is_arcade_mode`.
    main_panel_arcade: bool,
    /// Effect-primitive renderer (billboards / beams / cones / shells)
    /// — the `CBillboard::SortEndDraw` + BBT texture table port.
    effects_renderer: crate::matrix_game::effects::effects_renderer::EffectsRenderer,
    /// Projectile + debris mesh renderer.
    effect_meshes: crate::matrix_game::effects::effects_renderer::EffectMeshRenderer,
    /// Landscape decals (craters / scorch marks).
    spots: crate::matrix_game::effects::landscape_spot::LandscapeSpots,
    /// Per-frame primitive queues (reused allocations).
    bb_queue: crate::matrix_lib::three_g::billboard::BillboardQueue,
    mesh_queue: crate::matrix_game::effects::explosion::MeshQueue,
    /// FPS counter — wall-clock time of the last log emit and number of
    /// frames since. Drained once a second from `RedrawRequested`.
    fps_last_log: f64,
    fps_frames: u32,
    /// Per-phase frame-time accumulators (seconds):
    /// [takt, graphic_takt, ui, sync_robots, sync_other, render].
    /// Drained with the FPS log so heavy-battle slowdowns are attributable.
    perf_acc: [f64; 6],
    /// CSound port — consumes the queued [`SndEvent`]s each frame
    /// (logic produces, the app loop mixes).
    sounds: crate::matrix_game::sound::SoundMixer,
}

pub struct App {
    state: Rc<RefCell<Option<AppState>>>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.borrow().is_some() {
            return;
        }

        let attrs = WindowAttributes::default().with_title("MatrixGame");
        // Host-supplied display settings (`SRobotsSettings` resolution).
        #[cfg(not(target_arch = "wasm32"))]
        let attrs = {
            let num = |var: &str| std::env::var(var).ok().and_then(|v| v.parse::<u32>().ok());
            let attrs = match (num("MG_WIDTH"), num("MG_HEIGHT")) {
                (Some(w), Some(h)) if w > 0 && h > 0 => {
                    attrs.with_inner_size(winit::dpi::LogicalSize::new(w, h))
                }
                _ => attrs,
            };
            if std::env::var("MG_FULLSCREEN").as_deref() == Ok("1") {
                attrs.with_fullscreen(Some(winit::window::Fullscreen::Borderless(None)))
            } else {
                attrs
            }
        };

        #[cfg(target_arch = "wasm32")]
        let attrs = {
            use wasm_bindgen::JsCast;
            use winit::platform::web::WindowAttributesExtWebSys;
            let canvas = web_sys::window()
                .unwrap()
                .document()
                .unwrap()
                .get_element_by_id("game-canvas")
                .unwrap()
                .unchecked_into::<web_sys::HtmlCanvasElement>();
            attrs.with_canvas(Some(canvas))
        };

        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("failed to create window"),
        );

        // Native: init synchronously
        #[cfg(not(target_arch = "wasm32"))]
        {
            let gfx = pollster::block_on(GfxContext::new(window.clone()));

            let (map, stor, matrix_data, pkg) = load_map();

            let map = Arc::new(map);
            let mut camera = Camera::new(gfx.config.width as f32 / gfx.config.height as f32);
            camera.apply_camera_config(&matrix_data);
            camera.set_map(map.world_width(), map.world_height());
            camera.set_aspect(gfx.config.width as f32, gfx.config.height as f32);
            camera.init_strategy_angle(map.camera_angle);
            if let Some(pos) = map.camera_pos {
                camera.set_xy_strategy(pos);
            }
            camera.set_terrain_sampler({
                let m = map.clone();
                Box::new(move |x, y| m.group_max_z_interpolated(x, y, f32::MAX))
            });
            camera.set_ground_sampler({
                let m = map.clone();
                Box::new(move |x, y| m.get_z(x, y))
            });

            let tex_reader = |path: &str| -> Option<Vec<u8>> {
                let key = path.to_uppercase();
                for candidate in [&key, &format!("{key}.PNG"), &format!("{key}.DDS")] {
                    if let Ok(data) = pkg.read_file(candidate) {
                        return Some(data);
                    }
                }
                None
            };
            let terrain = MapRenderer::new(
                &gfx.device,
                &gfx.queue,
                &gfx.config,
                &map,
                &stor,
                &matrix_data,
                &tex_reader,
            );
            let point_lights = PointLightSystem::new(&map);
            let mut minimap = Minimap::new(
                &gfx.device,
                &gfx.queue,
                gfx.config.format,
                &map,
                &matrix_data,
                &tex_reader,
            );
            minimap.set_angle(-map.camera_angle);

            let mut game = MapLogic::new();
            game.load_config(&matrix_data);
            let building_ids = game.spawn_buildings(&map);
            game.spawn_ruins(&map);
            log::info!("world: spawned {} buildings", building_ids.len());
            let robot_ids = game.spawn_robots(&map);
            log::info!("world: spawned {} initial robots", robot_ids.len());
            game.ensure_sides_from_objects();
            game.apply_side_resources(&map);
            game.init_effect_spawners(&map);
            // Load-time `LogicTakt(100000)` fast-forward
            // (MatrixMapPrepare.cpp:1945): every owned building pays
            // one income tick at t=0.
            game.accrue_resources(100_000);
            select_start_base(&mut game, &mut camera, &map);
            let (_ids, stats) = game.spawn_map_objects(&map, &stor);
            log::info!(
                "world: spawned {} map objects (static={}, burn={}, break={}, anim={}, sens={}, spawner={}, terron={}, portret={}, special={})",
                stats.total(), stats.r#static, stats.burn, stats.r#break, stats.anim,
                stats.sens, stats.spawner, stats.terron, stats.portret, stats.special_win_target,
            );
            log::info!(
                "world: {} map objects enrolled in logic-temp list at init",
                game.objects.iter_logic().count(),
            );

            let selection_ring = crate::matrix_game::effects::selection::SelectionRingRenderer::new(
                &gfx.device,
                &gfx.config,
            );
            let marquee =
                crate::matrix_game::multi_selection::MarqueeRenderer::new(&gfx.device, &gfx.config);
            let move_to = crate::matrix_game::effects::move_to::MoveToRenderer::new();

            let mut iface_list =
                crate::matrix_game::interface::IFaceList::load_default_panels(&matrix_data);
            let map_texts = host_map_texts().or_else(|| {
                std::fs::read("assets/menu/maps.txt")
                    .ok()
                    .and_then(|b| String::from_utf8(b).ok())
                    .and_then(|raw| parse_map_texts(&raw, &briefing_stem()))
            });
            seed_mission_briefing(&mut iface_list, &game, map_texts.as_ref());
            log::info!("iface: loaded {} panels", iface_list.panels.len());
            let mut iface_renderer = crate::matrix_game::interface::InterfaceRenderer::new(
                &gfx.device,
                &gfx.queue,
                &gfx.config,
            );
            // Preload every atlas referenced by any element of any
            // loaded panel. `if/Main` alone pulls from interface1/2/3,
            // base_1, base_4, text_1; other panels add base_2/3/5/6.
            let pkg_ref = &pkg;
            let read = |p: &str| -> Option<Vec<u8>> {
                let key = p.replace('\\', "/").to_uppercase();
                for candidate in [&key, &format!("{key}.PNG")] {
                    if let Ok(data) = pkg_ref.read_file(candidate) {
                        return Some(data);
                    }
                }
                None
            };
            iface_renderer.preload_for_panels(
                &gfx.device,
                &gfx.queue,
                &read,
                iface_list.panels.iter(),
            );
            // Port of `CMatrixHint::PreloadBitmaps` (MatrixHint.cpp:
            // 441-459) — load the 9-slice border PNG + every alias in
            // `Hints/Bitmaps` (res_titan / res_energy / face_N / …).
            iface_renderer.preload_hint_chrome(
                &gfx.device,
                &gfx.queue,
                &read,
                &iface_list.hint_chrome,
            );
            let matrix_cursor = init_matrix_cursor(
                &matrix_data,
                &mut iface_renderer,
                &gfx.device,
                &gfx.queue,
                &read,
                &window,
            );
            let mut progress_bars = crate::matrix_game::progress_bar::ProgressBarRenderer::new(
                &gfx.device,
                &gfx.config,
            );
            progress_bars.load_atlas(&gfx.device, &gfx.queue, &read);

            // Port of `CIFaceList::ConstructorButtonsInit` (MatrixGame.cpp:517).
            // Seeds the constructor with default Pneumatic chassis +
            // ARMOR_6 + Machinegun so pylons show real components on
            // first open instead of "N/A" placeholders.
            if let Some(b) = game.player_side.builder.as_mut() {
                b.constructor_buttons_init();
            }

            let pause_overlay =
                crate::matrix_game::pause_overlay::PauseOverlay::new(&gfx.device, &gfx.config);
            let effects_renderer =
                crate::matrix_game::effects::effects_renderer::EffectsRenderer::new(
                    &gfx.device,
                    &gfx.queue,
                    &gfx.config,
                    crate::matrix_lib::three_g::texture::DEPTH_FORMAT,
                    &matrix_data,
                    &tex_reader,
                );
            let effect_meshes =
                crate::matrix_game::effects::effects_renderer::EffectMeshRenderer::new(
                    &gfx.device,
                    &gfx.queue,
                    &gfx.config,
                    crate::matrix_lib::three_g::texture::DEPTH_FORMAT,
                    &matrix_data,
                    &tex_reader,
                );
            game.objects.debris_catalog_len = effect_meshes.debris_count();
            game.objects.debris_types = effect_meshes.debris_types().to_vec();
            *self.state.borrow_mut() = Some(AppState {
                window,
                gfx,
                map,
                point_lights,
                light_follow_map: std::collections::HashMap::new(),
                terrain,
                minimap,
                camera,
                game,
                last_time: crate::platform::now_secs(),
                cursor: [-1.0, -1.0],
                matrix_cursor,
                mouse_trace: crate::matrix_game::map_trace::TraceStop::None,
                mouse_trace_pos: glam::Vec3::ZERO,
                minimap_dragging: false,
                shift_down: false,
                lmb_anchor: None,
                last_click: None,
                lmb_consumed_by_ui: false,
                marquee_last_rect: None,
                selection_ring,
                marquee,
                move_to,
                iface_list,
                iface_renderer,
                progress_bars,
                builder_preview: crate::matrix_game::interface::constructor::BuilderPreview::new(),
                robot_icons: crate::matrix_game::interface::RobotIconCache::new(),
                pause_overlay,
                effects_renderer,
                effect_meshes,
                spots: Default::default(),
                bb_queue: Default::default(),
                mesh_queue: Default::default(),
                is_paused: false,
                dialog: None,
                exit_state: 0,
                consumed_keys: Default::default(),
                cheat_keys: String::new(),
                fly_cam: None,
                held_keys: Default::default(),
                lmb_down: false,
                main_panel_arcade: false,
                fps_last_log: crate::platform::now_secs(),
                fps_frames: 0,
                perf_acc: [0.0; 6],
                sounds: crate::matrix_game::sound::SoundMixer::new(
                    crate::matrix_game::config::SoundDefs::from_matrix_data(&matrix_data),
                    crate::platform::audio::make_output(),
                ),
            });
            // Mission-begin dialog at end of load (MatrixMapPrepare.cpp:
            // 1956) — game paused behind it until dismissed. Skipped
            // when the data ships no "Begin" template (avoids an
            // undismissable empty dialog).
            if let Some(state) = self.state.borrow_mut().as_mut() {
                if !state
                    .iface_list
                    .hint_templates
                    .dialog_templates("Begin")
                    .is_empty()
                {
                    enter_dialog_mode(state, "Begin");
                }
            }
        }

        // WASM: init asynchronously
        #[cfg(target_arch = "wasm32")]
        {
            let state_slot = self.state.clone();
            let win = window;
            wasm_bindgen_futures::spawn_local(async move {
                let mut gfx = GfxContext::new(win.clone()).await;

                let (map, stor, matrix_data, bundle) = load_map_async().await;
                let map = Arc::new(map);

                let mut camera = Camera::new(gfx.config.width as f32 / gfx.config.height as f32);
                camera.apply_camera_config(&matrix_data);
                camera.set_map(map.world_width(), map.world_height());
                camera.set_aspect(gfx.config.width as f32, gfx.config.height as f32);
                camera.init_strategy_angle(map.camera_angle);
                if let Some(pos) = map.camera_pos {
                    camera.set_xy_strategy(pos);
                }
                camera.set_terrain_sampler({
                    let m = map.clone();
                    Box::new(move |x, y| m.group_max_z_interpolated(x, y, f32::MAX))
                });
                camera.set_ground_sampler({
                    let m = map.clone();
                    Box::new(move |x, y| m.get_z(x, y))
                });

                let tex_reader =
                    |path: &str| -> Option<Vec<u8>> { bundle.read_file(path).map(|s| s.to_vec()) };
                let mut terrain = MapRenderer::new(
                    &gfx.device,
                    &gfx.queue,
                    &gfx.config,
                    &map,
                    &stor,
                    &matrix_data,
                    &tex_reader,
                );
                let point_lights = PointLightSystem::new(&map);
                let mut minimap = Minimap::new(
                    &gfx.device,
                    &gfx.queue,
                    gfx.config.format,
                    &map,
                    &matrix_data,
                    &tex_reader,
                );
                minimap.set_angle(-map.camera_angle);

                // Force resize to match actual canvas dimensions
                let size = win.inner_size();
                log::debug!(
                    "wasm init: window inner_size = {}x{}",
                    size.width,
                    size.height
                );
                log::debug!(
                    "wasm init: surface config = {}x{}",
                    gfx.config.width,
                    gfx.config.height
                );
                if size.width > 0 && size.height > 0 {
                    gfx.resize(size.width, size.height);
                    terrain.resize(&gfx.device, &gfx.config);
                    // set_aspect mirrors the *surface* size so cursor (which
                    // we rescale from winit coords on the fly) and edge-pan
                    // thresholds share the same coord system.
                    camera.set_aspect(gfx.config.width as f32, gfx.config.height as f32);
                }

                let mut game = MapLogic::new();
                game.load_config(&matrix_data);
                let building_ids = game.spawn_buildings(&map);
                game.spawn_ruins(&map);
                log::info!("world: spawned {} buildings", building_ids.len());
                let robot_ids = game.spawn_robots(&map);
                log::info!("world: spawned {} initial robots", robot_ids.len());
                game.ensure_sides_from_objects();
                game.apply_side_resources(&map);
                game.init_effect_spawners(&map);
                game.accrue_resources(100_000);
                select_start_base(&mut game, &mut camera, &map);
                let (_ids, stats) = game.spawn_map_objects(&map, &stor);
                log::info!(
                    "world: spawned {} map objects (static={}, burn={}, break={}, anim={}, sens={}, spawner={}, terron={}, portret={}, special={})",
                    stats.total(), stats.r#static, stats.burn, stats.r#break, stats.anim,
                    stats.sens, stats.spawner, stats.terron, stats.portret, stats.special_win_target,
                );
                log::info!(
                    "world: {} map objects enrolled in logic-temp list at init",
                    game.objects.iter_logic().count(),
                );

                let selection_ring =
                    crate::matrix_game::effects::selection::SelectionRingRenderer::new(
                        &gfx.device,
                        &gfx.config,
                    );
                let marquee = crate::matrix_game::multi_selection::MarqueeRenderer::new(
                    &gfx.device,
                    &gfx.config,
                );
                let move_to = crate::matrix_game::effects::move_to::MoveToRenderer::new();

                let mut iface_list =
                    crate::matrix_game::interface::IFaceList::load_default_panels(&matrix_data);
                let map_texts = load_map_texts().await;
                seed_mission_briefing(&mut iface_list, &game, map_texts.as_ref());
                log::info!("iface: loaded {} panels", iface_list.panels.len());
                let mut iface_renderer = crate::matrix_game::interface::InterfaceRenderer::new(
                    &gfx.device,
                    &gfx.queue,
                    &gfx.config,
                );
                let read = |p: &str| -> Option<Vec<u8>> { bundle.read_file(p).map(|b| b.to_vec()) };
                iface_renderer.preload_for_panels(
                    &gfx.device,
                    &gfx.queue,
                    &read,
                    iface_list.panels.iter(),
                );
                iface_renderer.preload_hint_chrome(
                    &gfx.device,
                    &gfx.queue,
                    &read,
                    &iface_list.hint_chrome,
                );
                let matrix_cursor = init_matrix_cursor(
                    &matrix_data,
                    &mut iface_renderer,
                    &gfx.device,
                    &gfx.queue,
                    &read,
                    &win,
                );
                let mut progress_bars = crate::matrix_game::progress_bar::ProgressBarRenderer::new(
                    &gfx.device,
                    &gfx.config,
                );
                progress_bars.load_atlas(&gfx.device, &gfx.queue, &read);

                // Port of `CIFaceList::ConstructorButtonsInit`
                // (MatrixGame.cpp:517).
                if let Some(b) = game.player_side.builder.as_mut() {
                    b.constructor_buttons_init();
                }

                let pause_overlay =
                    crate::matrix_game::pause_overlay::PauseOverlay::new(&gfx.device, &gfx.config);
                let effects_renderer =
                    crate::matrix_game::effects::effects_renderer::EffectsRenderer::new(
                        &gfx.device,
                        &gfx.queue,
                        &gfx.config,
                        crate::matrix_lib::three_g::texture::DEPTH_FORMAT,
                        &matrix_data,
                        &read,
                    );
                let effect_meshes =
                    crate::matrix_game::effects::effects_renderer::EffectMeshRenderer::new(
                        &gfx.device,
                        &gfx.queue,
                        &gfx.config,
                        crate::matrix_lib::three_g::texture::DEPTH_FORMAT,
                        &matrix_data,
                        &read,
                    );
                game.objects.debris_catalog_len = effect_meshes.debris_count();
                game.objects.debris_types = effect_meshes.debris_types().to_vec();
                *state_slot.borrow_mut() = Some(AppState {
                    window: win.clone(),
                    gfx,
                    map,
                    point_lights,
                    light_follow_map: std::collections::HashMap::new(),
                    terrain,
                    minimap,
                    camera,
                    game,
                    last_time: crate::platform::now_secs(),
                    cursor: [-1.0, -1.0],
                    matrix_cursor,
                    mouse_trace: crate::matrix_game::map_trace::TraceStop::None,
                    mouse_trace_pos: glam::Vec3::ZERO,
                    minimap_dragging: false,
                    shift_down: false,
                    lmb_anchor: None,
                    last_click: None,
                    lmb_consumed_by_ui: false,
                    marquee_last_rect: None,
                    selection_ring,
                    marquee,
                    move_to,
                    iface_list,
                    iface_renderer,
                    progress_bars,
                    builder_preview:
                        crate::matrix_game::interface::constructor::BuilderPreview::new(),
                    robot_icons: crate::matrix_game::interface::RobotIconCache::new(),
                    pause_overlay,
                    effects_renderer,
                    effect_meshes,
                    spots: Default::default(),
                    bb_queue: Default::default(),
                    mesh_queue: Default::default(),
                    is_paused: false,
                    dialog: None,
                    exit_state: 0,
                    consumed_keys: Default::default(),
                    cheat_keys: String::new(),
                    fly_cam: None,
                    held_keys: Default::default(),
                    lmb_down: false,
                    main_panel_arcade: false,
                    fps_last_log: crate::platform::now_secs(),
                    fps_frames: 0,
                    perf_acc: [0.0; 6],
                sounds: crate::matrix_game::sound::SoundMixer::new(
                    crate::matrix_game::config::SoundDefs::from_matrix_data(&matrix_data),
                    crate::platform::audio::make_output(),
                ),
                });
                // Mission-begin dialog (MatrixMapPrepare.cpp:1956).
                if let Some(state) = state_slot.borrow_mut().as_mut() {
                    if !state
                        .iface_list
                        .hint_templates
                        .dialog_templates("Begin")
                        .is_empty()
                    {
                        enter_dialog_mode(state, "Begin");
                    }
                }
                win.request_redraw();
                hide_loading_overlay();
            });
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let mut state_ref = self.state.borrow_mut();
        let state = match state_ref.as_mut() {
            Some(s) => s,
            None => return,
        };

        match event {
            WindowEvent::CloseRequested => {
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(state) = self.state.borrow().as_ref() {
                    write_host_result(1, &state.game);
                }
                event_loop.exit()
            }

            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    state.gfx.resize(size.width, size.height);
                    state.terrain.resize(&state.gfx.device, &state.gfx.config);
                    state.camera.set_aspect(
                        state.gfx.config.width as f32,
                        state.gfx.config.height as f32,
                    );
                }
            }

            // ── Mouse input (MatrixFormGame.cpp:530-642) ──
            // Middle button toggles MouseCam mode (rotate-on-drag,
            // MatrixFormGame.cpp:631-642 — VK_MBUTTON only). Right-click
            // issues move orders to the selected robots
            // (C++: `CMatrixSideUnit::OnRButtonDown`).
            WindowEvent::MouseInput {
                state: btn_state,
                button,
                ..
            } => {
                use winit::event::{ElementState, MouseButton};
                let [cx, cy] = state.cursor;
                let w = state.gfx.config.width as f32;
                let h = state.gfx.config.height as f32;
                // ── Dialog mode swallows all world + panel input; only
                // the Hints-panel buttons stay live. The C++ reaches the
                // same state via MMFLAG_DIALOG_MODE + Pause(true): world
                // clicks die in `CMultiSelection::Begin`
                // (MatrixMultiSelection.cpp:34) and the selection was
                // dropped by PLDropAllActions, so the regular panels
                // have nothing to press.
                if state.dialog.is_some() {
                    if button == MouseButton::Left {
                        match btn_state {
                            ElementState::Pressed => {
                                let on_hints = state
                                    .iface_list
                                    .hit_test(cx, cy, w, h)
                                    .map(|(pi, _)| state.iface_list.panels[pi].name == "Hints")
                                    .unwrap_or(false);
                                if on_hints {
                                    state.iface_list.on_mouse_down(cx, cy, w, h);
                                }
                            }
                            ElementState::Released => {
                                if let Some(crate::matrix_game::interface::Click::Button(name)) =
                                    state.iface_list.on_mouse_up(cx, cy, w, h)
                                {
                                    press_hint_button(state, &name);
                                }
                            }
                        }
                    }
                    return;
                }
                if button == MouseButton::Middle {
                    let pressed = btn_state == ElementState::Pressed;
                    state.camera.on_rotate_button(pressed, cx, cy);
                } else if button == MouseButton::Right {
                    // RMB → UI first (CIFaceButton::OnMouseRBDown opens
                    // the constructor popup menu when a pylon catches
                    // the press). Move-orders only run if no UI element
                    // claims the event.
                    let ui_consumed_rmb = match btn_state {
                        ElementState::Pressed => state.iface_list.on_mouse_right_down(cx, cy, w, h),
                        ElementState::Released => state
                            .iface_list
                            .on_mouse_right_up(cx, cy, w, h)
                            .map(|click| {
                                log::debug!("iface: right-clicked {:?}", click);
                                dispatch_ui_right_click(state, &click);
                            })
                            .is_some(),
                    };
                    // World move-orders are gated on the cursor not being
                    // over any interface element or the minimap — port of
                    // the `g_IFaceList->m_InFocus == UNKNOWN` guard at
                    // MatrixFormGame.cpp:756-760.
                    let over_ui = state.iface_list.hit_test(cx, cy, w, h).is_some()
                        || state.minimap.click_to_world(cx, cy).is_some();
                    // RMB cancels only a committed turret placement (ghost
                    // exists): `IS_PREORDERING && BUILDING_TURRET` at
                    // MatrixSide.cpp:804-810. Armed pre-orders stay armed —
                    // OnRButtonDown gates every order branch on
                    // !IS_PREORDERING and never clears the flag; the picker
                    // without a kind likewise swallows the click.
                    if !ui_consumed_rmb
                        && btn_state == ElementState::Pressed
                        && state.iface_list.turret_build.is_active()
                    {
                        if state.iface_list.turret_build.kind.is_some() {
                            state.iface_list.turret_build.cancel();
                            log::debug!("turret placement: cancelled by right-click");
                        }
                    } else if !ui_consumed_rmb
                        && btn_state == ElementState::Pressed
                        && state.iface_list.pre_order.is_some()
                    {
                        // Armed pre-order: RMB is inert (MatrixSide.cpp:835).
                    } else if !ui_consumed_rmb
                        && btn_state == ElementState::Pressed
                        && !state.game.is_arcade_mode()
                        && !state.game.player_side.selected.is_empty()
                        && state.minimap.click_to_world(cx, cy).is_some()
                    {
                        // Right-click on the minimap → move order at the
                        // minimap world position + red ping
                        // (MatrixSide.cpp:821-830).
                        if let Some(tgt) = state.minimap.click_to_world(cx, cy) {
                            state
                                .game
                                .order_move_to_world(&state.map, glam::Vec2::new(tgt[0], tgt[1]));
                            state
                                .minimap
                                .add_event(tgt[0], tgt[1], 0xffff0000, 0xffff0000);
                        }
                    } else if !ui_consumed_rmb
                        && !over_ui
                        && btn_state == ElementState::Pressed
                        && !state.game.is_arcade_mode()
                        && !state.game.player_side.selected.is_empty()
                    {
                        // Full OnRButtonDown dispatch (MatrixSide.cpp:
                        // 799-863): enemy building → capture, enemy
                        // unit → attack, else move. `OnRButtonDown`
                        // early-outs while arcaded (MatrixSide.cpp:806).
                        state
                            .game
                            .on_right_click(&state.camera, cx, cy, w, h, &state.map);
                    }
                } else if button == MouseButton::Left {
                    use crate::matrix_game::minimap::MinimapClick;
                    // Raw LMB state — the `GetAsyncKeyState(KA_FIRE)`
                    // poll the arcaded robot's fire branch reads.
                    state.lmb_down = btn_state == ElementState::Pressed;
                    match btn_state {
                        ElementState::Pressed => {
                            // UI first dibs (MatrixFormGame.cpp:748-755).
                            if state.iface_list.on_mouse_down(cx, cy, w, h) {
                                if let Some(cfg) = state.iface_list.popup_restore_pending.take() {
                                    if let Some(b) = state.game.player_side.builder.as_mut() {
                                        b.apply_config(cfg);
                                    }
                                    reset_build_counter(state);
                                }
                                state.minimap_dragging = false;
                                state.lmb_anchor = None;
                                state.lmb_consumed_by_ui = true;
                            } else {
                                if let Some(cfg) = state.iface_list.popup_restore_pending.take() {
                                    if let Some(b) = state.game.player_side.builder.as_mut() {
                                        b.apply_config(cfg);
                                    }
                                    reset_build_counter(state);
                                }
                                match state.minimap.click(cx, cy) {
                                    MinimapClick::BeginDrag(tgt) => {
                                        // Armed pre-order + minimap click →
                                        // execute the order at the minimap
                                        // world position instead of
                                        // recentring (MatrixSide.cpp:672-680).
                                        if !execute_pre_order_world(state, tgt) {
                                            state.camera.set_xy_strategy(tgt);
                                            state.minimap_dragging = true;
                                        }
                                        state.lmb_anchor = None;
                                        state.lmb_consumed_by_ui = true;
                                    }
                                    MinimapClick::ZoomIn | MinimapClick::ZoomOut => {
                                        state.minimap_dragging = false;
                                        state.lmb_anchor = None;
                                        state.lmb_consumed_by_ui = true;
                                    }
                                    MinimapClick::None => {
                                        state.minimap_dragging = false;
                                        // Record the press anchor. The
                                        // click-vs-marquee decision is
                                        // deferred until release, matching
                                        // `CMultiSelection::Begin` +
                                        // `End` semantics (MatrixFormGame.
                                        // cpp:763-770, 664-670).
                                        //
                                        // Pause guard mirrors
                                        // `CMultiSelection::Begin` at
                                        // MatrixMultiSelection.cpp:34
                                        // returning NULL when the map is
                                        // paused — so while the constructor
                                        // is open (Pause(true) inside
                                        // CConstructorPanel::ActivateAndSelect,
                                        // CConstructor.cpp:975) world
                                        // clicks neither click-select
                                        // nor marquee, leaving the
                                        // constructor open until the
                                        // user presses `cocan`.
                                        // Arcade mode: no click-select /
                                        // marquee (MatrixFormGame.cpp:763
                                        // gates on !IsArcadeMode()) — LMB
                                        // is the fire trigger.
                                        if !state.is_paused && !state.game.is_arcade_mode() {
                                            state.lmb_anchor = Some([cx, cy]);
                                            state.lmb_consumed_by_ui = false;
                                        } else {
                                            state.lmb_anchor = None;
                                            state.lmb_consumed_by_ui = true;
                                        }
                                    }
                                }
                            }
                        }
                        ElementState::Released => {
                            state.minimap_dragging = false;
                            if let Some(click) = state.iface_list.on_mouse_up(cx, cy, w, h) {
                                log::debug!("iface: clicked {:?}", click);
                                dispatch_ui_click(state, &click);
                            } else if state.iface_list.turret_build.is_active()
                                && state.iface_list.turret_build.kind.is_some()
                            {
                                // Turret placement click — only fires
                                // once a kind has been committed via
                                // tur1..4. Picker-only state (kind
                                // None) leaves world clicks alone so
                                // the user can still interact with
                                // the rest of the UI / world.
                                state.lmb_anchor = None;
                                try_place_turret(state, cx, cy, w, h);
                            } else if state.iface_list.pre_order.is_some()
                                && !state.lmb_consumed_by_ui
                            {
                                // Armed robot order — this click is its
                                // target (the PREORDER_* execution at
                                // MatrixSide.cpp:702-727).
                                state.lmb_anchor = None;
                                execute_pre_order(state, cx, cy, w, h);
                            } else if let Some([ax, ay]) = state.lmb_anchor.take() {
                                if !state.lmb_consumed_by_ui {
                                    // Drag distance — anything ≤ 4 px is a
                                    // click, otherwise a marquee rect.
                                    let dx = (cx - ax).abs();
                                    let dy = (cy - ay).abs();
                                    const DRAG_PX: f32 = 4.0;
                                    if dx <= DRAG_PX && dy <= DRAG_PX {
                                        // Double-click → select all own
                                        // robots in radius (OnLButtonDouble).
                                        let now_ms = state.game.elapsed_ms;
                                        let dbl = state
                                            .last_click
                                            .map(|(t, px, py)| {
                                                now_ms - t < 350
                                                    && (px - cx).abs() <= DRAG_PX
                                                    && (py - cy).abs() <= DRAG_PX
                                            })
                                            .unwrap_or(false);
                                        state.last_click = Some((now_ms, cx, cy));
                                        if dbl
                                            && state.game.on_left_double_click(
                                                &state.camera,
                                                cx,
                                                cy,
                                                w,
                                                h,
                                            )
                                        {
                                            state.lmb_consumed_by_ui = false;
                                            state.lmb_anchor = None;
                                            return;
                                        }
                                        let hit = state.game.click_at_screen(
                                            &state.camera,
                                            cx,
                                            cy,
                                            w,
                                            h,
                                            state.shift_down,
                                        );
                                        match hit {
                                            Some(id) => log::debug!(
                                                "selection: hit object {:?}, curr_sel={:?}, selected={}",
                                                id, state.game.player_side.curr_sel,
                                                state.game.player_side.selected.len(),
                                            ),
                                            None => log::debug!(
                                                "selection: cleared (selected={})",
                                                state.game.player_side.selected.len(),
                                            ),
                                        }
                                        // Mirror into the CMatrixGroup
                                        // machinery the PGOrder layer
                                        // dispatches on.
                                        state.game.sync_group_from_selection();
                                    } else {
                                        let rmin = [ax.min(cx), ay.min(cy)];
                                        let rmax = [ax.max(cx), ay.max(cy)];
                                        let n = crate::matrix_game::multi_selection::marquee_select(
                                            &mut state.game,
                                            &state.camera,
                                            rmin,
                                            rmax,
                                            w,
                                            h,
                                            state.shift_down,
                                        );
                                        log::debug!("marquee: selected {} robot(s)", n,);
                                        state.game.sync_group_from_selection();
                                        // Start the 50ms DIP fade — port
                                        // of `CMultiSelection::End` at
                                        // MatrixMultiSelection.cpp:278-279.
                                        state.marquee.begin_dip_fade(state.game.elapsed_ms as f32);
                                    }
                                }
                            }
                            state.lmb_consumed_by_ui = false;
                        }
                    }
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                // winit reports cursor in its own "physical" coord system
                // (CSS × DPR). When the surface is clamped to
                // `max_texture_dimension_2d` (2048 on WebGL2) but the viewport
                // is larger, surface coords are a *subset* of winit coords —
                // rendering and hit-tests use the smaller surface coords, so
                // rescale cursor to the surface coord system here.
                let win_size = state.window.inner_size();
                let sx = if win_size.width > 0 {
                    state.gfx.config.width as f64 / win_size.width as f64
                } else {
                    1.0
                };
                let sy = if win_size.height > 0 {
                    state.gfx.config.height as f64 / win_size.height as f64
                } else {
                    1.0
                };
                let cx = (position.x * sx) as f32;
                let cy = (position.y * sy) as f32;
                state.cursor = [cx, cy];
                if state.minimap_dragging {
                    if let Some(tgt) = state.minimap.click_to_world(cx, cy) {
                        state.camera.set_xy_strategy(tgt);
                    }
                }
                // Live marquee rect — update while LMB is held and
                // the drag has exceeded the click threshold. Ports the
                // `CMultiSelection::Update` call at MatrixFormGame.cpp:
                // 564 that re-evaluates the rect each mouse move.
                if let Some([ax, ay]) = state.lmb_anchor {
                    if !state.lmb_consumed_by_ui && state.iface_list.pre_order.is_none() {
                        const DRAG_PX: f32 = 4.0;
                        if (cx - ax).abs() > DRAG_PX || (cy - ay).abs() > DRAG_PX {
                            let rect = [ax, ay, cx, cy];
                            state.marquee_last_rect = Some(rect);
                            let w = state.gfx.config.width as f32;
                            let h = state.gfx.config.height as f32;
                            state.marquee.set_rect(&state.gfx.queue, rect, w, h);
                            // Live re-selection each move so boxed units
                            // highlight during the drag (CMultiSelection::
                            // Update, MatrixFormGame.cpp:564). Skipped while
                            // Shift is held so the additive union is applied
                            // once at release against the original selection.
                            if !state.shift_down {
                                let rmin = [ax.min(cx), ay.min(cy)];
                                let rmax = [ax.max(cx), ay.max(cy)];
                                crate::matrix_game::multi_selection::marquee_select(
                                    &mut state.game,
                                    &state.camera,
                                    rmin,
                                    rmax,
                                    w,
                                    h,
                                    false,
                                );
                                state.game.sync_group_from_selection();
                            }
                        }
                    }
                }
                // Interface hover-state tracking — port of
                // `CIFaceList::OnMouseMove` (Interface/CInterface.cpp).
                {
                    let w = state.gfx.config.width as f32;
                    let h = state.gfx.config.height as f32;
                    let (unfocused, focused) = state.iface_list.on_mouse_move(cx, cy, w, h);
                    let mut preview_changed = false;
                    if let Some(b) = state.game.player_side.builder.as_mut() {
                        preview_changed = preview_popup_hover(b, state.iface_list.popup.as_mut());
                    }
                    if preview_changed {
                        reset_build_counter(state);
                    }
                    // Route Base-panel focus changes into the
                    // constructor — port of CConstructor.cpp:903-958
                    // (`RemoteFocusElement` / `RemoteUnFocusElement`).
                    //
                    // While the popup is open the C++ skips OnMouseMove
                    // for buttons (CInterface.cpp:979) and additionally
                    // RemoteUnFocusElement guards on `POPUP_MENU_ACTIVE`
                    // (CConstructor.cpp:907-910). Net effect: pylon
                    // hover changes are frozen until the popup closes.
                    let popup_active = state.iface_list.popup.is_some();
                    if !popup_active {
                        if let Some(b) = state.game.player_side.builder.as_mut() {
                            if let Some((panel, elem)) = unfocused {
                                if panel == "Base" {
                                    b.unfocus_element(&elem);
                                }
                            }
                            if let Some((panel, elem)) = focused {
                                if panel == "Base" {
                                    b.focus_element(&elem);
                                }
                            }
                        }
                    }
                }
                // Mouse-cam drag also steers the arcaded robot's
                // chassis by the horizontal delta (MatrixFormGame.cpp:
                // 536-546).
                if state.camera.is_mouse_cam() {
                    if let Some(aid) = state.game.objects.arcaded_object {
                        let dx = cx - state.camera.last_mouse_x();
                        if dx != 0.0 {
                            use crate::matrix_game::map_static::{
                                ROBOT_FLAG_ROT_LEFT, ROBOT_FLAG_ROT_RIGHT,
                            };
                            if let Some(r) =
                                crate::matrix_game::logic::robot_mut(&mut state.game.objects, aid)
                            {
                                use crate::matrix_game::map_static::MapStatic;
                                r.object_state_set(if dx < 0.0 {
                                    ROBOT_FLAG_ROT_LEFT
                                } else {
                                    ROBOT_FLAG_ROT_RIGHT
                                });
                            }
                        }
                    }
                }
                state.camera.on_mouse_move(cx, cy);
            }

            WindowEvent::CursorLeft { .. } => {
                state.cursor = [-1.0, -1.0];
                state.camera.on_cursor_left();
            }

            WindowEvent::MouseWheel { delta, .. } => {
                // Each wheel notch = one ZoomInStep/OutStep call.
                use winit::event::MouseScrollDelta;
                let notches = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 120.0,
                };
                state.camera.on_mouse_wheel(notches);
            }

            // ── Keyboard (MatrixFormGame.cpp:247-282) ──
            WindowEvent::KeyboardInput { event, .. } => {
                use crate::matrix_game::camera::KeyAction;
                use crate::matrix_game::keybinds::{self, Ctx};
                use winit::keyboard::{KeyCode, PhysicalKey};
                let pressed = event.state == winit::event::ElementState::Pressed;
                // User remaps rewrite a physical key to its action's
                // canonical key, per context — camera and game defaults
                // share keys (A/S/D), so each dispatch below gets its
                // own translation. None = canonical key rebound away.
                let cam_code = match event.physical_key {
                    PhysicalKey::Code(c) => keybinds::map(Ctx::Camera, c),
                    _ => None,
                };
                // Game-key dispatch first (MatrixFormGame.cpp:1172-1560);
                // a consumed key never reaches the camera bindings, and
                // its auto-repeats are swallowed too — toggles (pause,
                // auto-orders) fire once per physical press and a held
                // `S` must not scroll the camera after stopping a group.
                if let PhysicalKey::Code(code) = event.physical_key {
                    // Cheat-word ring (MatrixFormGame.cpp:888-947):
                    // typing FLYCAM toggles the spectator autopilot.
                    // Recorded before any dispatch, like `m_LastScans`.
                    if pressed && !event.repeat {
                        let name = format!("{code:?}");
                        if let Some(letter) = name.strip_prefix("Key").filter(|s| s.len() == 1) {
                            state.cheat_keys.push_str(letter);
                            if state.cheat_keys.len() > 6 {
                                let cut = state.cheat_keys.len() - 6;
                                state.cheat_keys.drain(..cut);
                            }
                            if state.cheat_keys == "FLYCAM" {
                                state.cheat_keys.clear();
                                state.game.objects.fly_cam = !state.game.objects.fly_cam;
                                log::info!(
                                    "flycam {}",
                                    if state.game.objects.fly_cam { "on" } else { "off" }
                                );
                                return;
                            }
                            // HURRY (MatrixFormGame.cpp:975-987):
                            // finish the reinforcements cooldown now.
                            if state.cheat_keys.ends_with("HURRY") {
                                state.cheat_keys.clear();
                                if !state.game.maintenance_disabled()
                                    && state.game.maintenance_time > 0
                                {
                                    state.game.maintenance_time = 1;
                                }
                                return;
                            }
                        }
                    }
                    // Held-state for the arcade `GetAsyncKeyState`
                    // polls — tracked before any consumption so the
                    // manual-drive keys can't stick. Uses the camera
                    // translation (arcade drive shares the camera keys).
                    if let Some(cc) = cam_code {
                        if matches!(
                            cc,
                            KeyCode::KeyW
                                | KeyCode::KeyA
                                | KeyCode::KeyS
                                | KeyCode::KeyD
                                | KeyCode::ArrowUp
                                | KeyCode::ArrowDown
                                | KeyCode::ArrowLeft
                                | KeyCode::ArrowRight
                        ) {
                            if pressed {
                                state.held_keys.insert(cc);
                            } else {
                                state.held_keys.remove(&cc);
                            }
                        }
                    }
                    if pressed {
                        // Dialog shortcuts (E/S/R/Enter/Esc) are fixed —
                        // skip the remap while a dialog is up.
                        let game_code = if state.dialog.is_some() {
                            Some(code)
                        } else {
                            keybinds::map(Ctx::Game, code)
                        };
                        if event.repeat {
                            if state.consumed_keys.contains(&code) {
                                return;
                            }
                        } else if game_code.is_some_and(|gc| handle_game_key(state, gc)) {
                            state.consumed_keys.insert(code);
                            return;
                        } else {
                            state.consumed_keys.remove(&code);
                        }
                    } else {
                        state.consumed_keys.remove(&code);
                    }
                }
                let action = match cam_code {
                    Some(KeyCode::ArrowUp) | Some(KeyCode::KeyW) => Some(KeyAction::MoveForward),
                    Some(KeyCode::ArrowDown) | Some(KeyCode::KeyS) => Some(KeyAction::MoveBack),
                    Some(KeyCode::ArrowLeft) | Some(KeyCode::KeyA) => Some(KeyAction::MoveLeft),
                    Some(KeyCode::ArrowRight) | Some(KeyCode::KeyD) => Some(KeyAction::MoveRight),
                    // Yaw (KA_ROTATE_LEFT/RIGHT). Original: Home/End + `[`/`]`.
                    Some(KeyCode::Home) | Some(KeyCode::BracketLeft) => Some(KeyAction::RotLeft),
                    Some(KeyCode::End) | Some(KeyCode::BracketRight) => Some(KeyAction::RotRight),
                    // Pitch (KA_ROTATE_UP/DOWN). Original: PageUp/PageDown.
                    Some(KeyCode::PageUp) => Some(KeyAction::RotUp),
                    Some(KeyCode::PageDown) => Some(KeyAction::RotDown),
                    // Reset angles (KA_CAM_SETDEFAULT). Original: `\`.
                    Some(KeyCode::Backslash) => Some(KeyAction::ResetAngles),
                    // Shift — tracked for click-to-toggle in the
                    // multi-selection path. Matches the C++ shift
                    // modifier branch in `CMultiSelection::Add`
                    // (MatrixSide.cpp:1584-1598).
                    Some(KeyCode::ShiftLeft) | Some(KeyCode::ShiftRight) => {
                        state.shift_down = pressed;
                        None
                    }
                    _ => None,
                };
                if let Some(a) = action {
                    state.camera.on_key(a, pressed);
                }
            }

            WindowEvent::RedrawRequested => {
                let now = crate::platform::now_secs();
                // Clamp each takt to 100 ms like the original render loop
                // (3g.cpp:425-433, `tt1 = min(100, cur_takt)`) — keeps a
                // tab-switch / debugger pause from fast-forwarding logic.
                let dt = ((now - state.last_time) as f32).min(0.1);
                state.last_time = now;

                state.fps_frames += 1;
                let fps_elapsed = now - state.fps_last_log;
                if fps_elapsed >= 1.0 {
                    let fps = state.fps_frames as f64 / fps_elapsed;
                    let n = state.fps_frames.max(1) as f64;
                    // Console perf line off by default; enable with ?log=debug.
                    // The on-screen FPS counter (set_fps_text below) stays.
                    log::debug!(
                        "fps: {:.1} ({} frames / {:.2}s) | per-frame ms: takt={:.2} gfx={:.2} ui={:.2} syncR={:.2} sync={:.2} render={:.2} | robots={} effects={} bb={}",
                        fps,
                        state.fps_frames,
                        fps_elapsed,
                        state.perf_acc[0] / n * 1000.0,
                        state.perf_acc[1] / n * 1000.0,
                        state.perf_acc[2] / n * 1000.0,
                        state.perf_acc[3] / n * 1000.0,
                        (state.perf_acc[4] - state.perf_acc[3]).max(0.0) / n * 1000.0,
                        state.perf_acc[5] / n * 1000.0,
                        state
                            .game
                            .objects
                            .iter_units()
                            .filter(|&id| crate::matrix_game::logic::robot_ref(&state.game.objects, id).is_some())
                            .count(),
                        state.game.effects.len(),
                        state.bb_queue.billboards.len(),
                    );
                    state.perf_acc = [0.0; 6];
                    crate::platform::set_fps_text(&format!("{fps:.0} FPS"));
                    state.fps_last_log = now;
                    state.fps_frames = 0;
                }
                // Logic takt first (ports `CMatrixMapLogic::Takt`, which
                // runs `ProceedLogic` before the graphic takt starts —
                // MatrixLogic.cpp:2722-2761). Then per-object graphic
                // takt (SortEndGraphicTakt, MatrixMapStatic.cpp:755-765).
                // Camera / minimap / terrain takts mirror
                // `CMatrixMap::Takt`'s remaining subsystem dispatches.
                let step_ms = (dt * 1000.0).round() as i32;
                {
                    // Scope the map pointer for the duration of
                    // the logic + graphic takt so dispatched
                    // `logic_takt` / `takt` methods can call
                    // `current_map()` (ports `g_MatrixMap`).
                    let _scope =
                        crate::matrix_game::map::MapScope::enter(&state.map, state.game.elapsed_ms);
                    // Arcade (in-robot) mode: refresh the manual-input
                    // snapshot + camera follow target and reconcile
                    // the Main-panel arcade slide.
                    sync_arcade_mode(state);
                    // Frustum-center for MAX_EFFECT_DISTANCE culling —
                    // the camera link point (follows the arcaded robot
                    // in arcade mode, the strategy target otherwise).
                    let (cx, cy) = if let Some(t) = state.game.objects.arcaded_object.and_then(
                        |id| crate::matrix_game::logic::get_world_pos(&state.game.objects, id),
                    ) {
                        (t.x, t.y)
                    } else {
                        state.camera.strategy_xy()
                    };
                    crate::matrix_game::map::set_frustum_center([cx, cy]);
                    // Port of the pause early-return in
                    // `CMatrixMapLogic::Takt` (MatrixLogic.cpp:2643):
                    // it fires BEFORE `CMatrixMap::Takt` (:2761), so
                    // while paused object animations, water, and effect
                    // timers all freeze — only rendering continues.
                    if !state.is_paused {
                        let tt = crate::platform::now_secs();
                        state.game.takt(step_ms);
                        let tg = crate::platform::now_secs();
                        state.game.graphic_takt(step_ms);
                        state.perf_acc[1] += crate::platform::now_secs() - tg;
                        state.perf_acc[0] += tg - tt;
                    }
                    // Landscape decals: age + build geometry for new
                    // spawns (CMatrixEffectLandscapeSpot list).
                    if !state.is_paused {
                        state.spots.takt(step_ms as f32);
                    }
                    let pending: Vec<_> = state.game.objects.pending_spots.drain(..).collect();
                    for sp in pending {
                        state.spots.spawn(&state.map, &sp);
                    }
                    // Buildings entering ROP_CAPTURING force-deselect if
                    // the player has them selected (MatrixRobot.cpp:
                    // 1286-1294: Select(NOTHING) + PLDropAllActions).
                    if !state.game.objects.pending_capture_deselect.is_empty() {
                        use crate::matrix_game::side::CurrSel;
                        let desel: Vec<_> = state
                            .game
                            .objects
                            .pending_capture_deselect
                            .drain(..)
                            .collect();
                        if matches!(
                            state.game.player_side.curr_sel,
                            CurrSel::BaseSelected | CurrSel::BuildingSelected
                        ) && state
                            .game
                            .player_side
                            .active_object
                            .is_some_and(|a| desel.contains(&a))
                        {
                            pl_drop_all_actions(state);
                        }
                    }
                    // Effect point-lights: register new flashes, then age
                    // the transient ones (CreatePointLight + fade).
                    let pending_lights: Vec<_> =
                        state.game.objects.pending_point_lights.drain(..).collect();
                    for pl in pending_lights {
                        state.point_lights.add_transient_light_anim(
                            &state.map, pl.pos, pl.r1, pl.r2, pl.c1, pl.c2, pl.ttl, pl.t1,
                        );
                    }
                    // Follow-lights (plasma bolt / flame glow): move an
                    // existing keyed light or create it; kill on effect death.
                    let follows: Vec<_> =
                        state.game.objects.pending_light_follow.drain(..).collect();
                    for (key, pos, radius, color) in follows {
                        if let Some(&id) = state.light_follow_map.get(&key) {
                            state.point_lights.set_pos(&state.map, id, pos);
                            state.point_lights.set_radius(&state.map, id, radius);
                            state.point_lights.set_color(&state.map, id, color);
                        } else {
                            let id = state.point_lights.add_light(&state.map, pos, radius, color);
                            state.light_follow_map.insert(key, id);
                        }
                    }
                    let kills: Vec<_> = state.game.objects.pending_light_kill.drain(..).collect();
                    for key in kills {
                        if let Some(id) = state.light_follow_map.remove(&key) {
                            state.point_lights.remove_light(&state.map, id);
                        }
                    }
                    if !state.is_paused {
                        state.point_lights.takt(&state.map, step_ms as f32);
                    }
                    // Single batched recompute after all light adds/moves/
                    // expiries this frame (no-op when nothing is dirty),
                    // rate-capped: every flush triggers a full terrain +
                    // instance relight downstream, which at battle-time
                    // light churn used to run every frame.
                    state
                        .point_lights
                        .flush_throttled(&state.map, state.game.elapsed_ms as f64, 50.0);
                }

                // Drain every queued sound event into the mixer —
                // runs during pause too (UI clicks stay audible).
                pump_sounds(state);

                // Win/Loose end-of-game dialog — the deferred
                // EnterDialogMode(TEMPLATE_DIALOG_WIN/LOOSE) call from
                // MatrixLogic.cpp:2900-2906 (logic queues, UI executes).
                if let Some(win) = state.game.pending_win_loose_dialog.take() {
                    if win {
                        save_map_record(state.game.elapsed_ms);
                    }
                    enter_dialog_mode(state, if win { "Win" } else { "Loose" });
                }

                // Reconcile + animate the selection-ring effect with
                // `player_side.active_object`. Ports the C++
                // CMatrixEffectSelection lifecycle (create on Select,
                // destroy on UnSelect, follow the object's geo-center
                // each frame, advance dot animation per takt).
                sync_selection_ring(state, step_ms as f32);

                // Advance the marquee's DIP fade (ports the per-frame
                // `CMultiSelection::Draw` fade block at
                // MatrixMultiSelection.cpp:109-123). When the rect is
                // not in DIP, this is a no-op.
                if state.marquee.is_fading() {
                    let w = state.gfx.config.width as f32;
                    let h = state.gfx.config.height as f32;
                    state.marquee.takt(
                        &state.gfx.queue,
                        state.game.elapsed_ms as f32,
                        state.marquee_last_rect,
                        w,
                        h,
                    );
                }

                // Drain the move-order pings PGShowPlace queued — the
                // CreateMoveto / DeleteAllMoveto pair (MatrixSide.cpp:
                // 8542-8562).
                if state.game.moveto_clear_pending {
                    state.game.moveto_clear_pending = false;
                    state.move_to.clear();
                }
                for p in std::mem::take(&mut state.game.moveto_pings) {
                    state.move_to.spawn(p);
                }

                // Per-frame move-order ping animation advance — port
                // of `CMatrixEffectMoveto::Takt` (MatrixEffectMoveTo.cpp:93).
                if state.move_to.is_active() {
                    state.move_to.takt(step_ms as f32);
                }

                // Robot mesh rendering is in place via
                // `state.terrain.sync_robots`, so the per-robot point
                // lights that used to stand in for visibility are no
                // longer needed. Keeping that helper alive cost ~40 FPS
                // because every moving robot bumped the point-light
                // revision, which forced `terrain.takt` to rewrite +
                // re-upload the entire terrain vertex-colour buffer
                // each frame. Left out unless we add real point lights
                // (e.g. weapon flashes) that justify the GPU sync cost.

                // Per-frame mouse trace (m_TraceStopObj/Pos) — inside
                // `CMatrixMap::Takt` in the C++, so it freezes while
                // paused and cursor logic below reads the frozen value.
                if !state.is_paused {
                    update_mouse_trace(state);
                }

                // Per-frame interface visibility dispatch — ports the
                // `CInterface::LogicTakt` branch at
                // CInterface.cpp:1214-1635 (Main / Base / Top gating).
                // Also selects the cursor mode (CInterface.cpp:2957).
                refresh_interface_visibility(state);

                // Cursor frame animation — runs during pause too
                // (MatrixMap.cpp:2518 and the paused branch at
                // MatrixLogic.cpp:2618 both call m_Cursor.Takt).
                state.matrix_cursor.takt(step_ms);
                state.matrix_cursor.pos = state.cursor;

                // Advance the constructor 3D preview turntable + emit
                // a preview draw-ticket while the constructor panel is
                // open. Stand-in for CConstructor.cpp:251-262 +
                // :264-360 (Render).
                tick_builder_preview(state, dt * 1000.0);

                // Drain a pending popup-close focus clear — when the
                // popup was dismissed by clicking outside it, the
                // focused pylon's right-side preview + price label
                // need to drop along with the popup. The C++ achieves
                // this naturally via the next OnMouseMove pass under
                // !POPUP_MENU_ACTIVE; we clear explicitly here so
                // there's no one-frame stale state.
                if state.iface_list.popup_focus_clear_pending {
                    state.iface_list.popup_focus_clear_pending = false;
                    if let Some(b) = state.game.player_side.builder.as_mut() {
                        if let Some(name) = b.focused_element.clone() {
                            b.unfocus_element(&name);
                        }
                    }
                }

                // Tooltip timer + dynamic hint text refresh. Port of
                // `CIFaceList::OnMouseMove` hint build pass at
                // CIFaceButton.cpp:134-145 combined with
                // `AddHintReplacements` (CInterface.cpp:4439-4540).
                refresh_hint_replacements(state);
                {
                    let w = state.gfx.config.width as f32;
                    let h = state.gfx.config.height as f32;
                    // Snapshot each loaded hint-bitmap's pixel
                    // dimensions up front so the hint layout engine
                    // can size inline resource icons without re-
                    // borrowing `iface_renderer` (which we need as
                    // &mut for the glyph atlas). The snapshot is
                    // cheap — there are ~30 alias entries.
                    let mut sizes: std::collections::HashMap<String, (i32, i32)> =
                        std::collections::HashMap::new();
                    for bmp in state.iface_list.hint_chrome.bitmaps.values() {
                        if let Some((w, h)) = state.iface_renderer.atlas_size(&bmp.path) {
                            sizes.insert(bmp.path.clone(), (w as i32, h as i32));
                        }
                    }
                    let sizer = |path: &str| sizes.get(path).copied();
                    let tu = crate::platform::now_secs();
                    state.iface_list.update(
                        dt * 1000.0,
                        w,
                        h,
                        state.iface_renderer.glyph_atlas_mut(),
                        &sizer,
                    );
                    state.perf_acc[2] += crate::platform::now_secs() - tu;
                }

                let t_logic_end = crate::platform::now_secs();

                // MMFLAG_FLYCAM branch of `CMatrixCamera::Takt`
                // (MatrixCamera.cpp:937-965): the autopilot owns the
                // camera and the normal input takt is skipped; the
                // AFD is freed as soon as the flag drops.
                if state.game.objects.fly_cam {
                    let pairs: Vec<_> = state.game.objects.pending_war_pairs.drain(..).collect();
                    let afd = state.fly_cam.get_or_insert_with(|| {
                        crate::matrix_game::camera::AutoFlyData::new(&state.camera)
                    });
                    for (target, attacker) in pairs {
                        afd.add_war_pair(target, attacker, &state.game.objects);
                    }
                    afd.takt(
                        dt * 1000.0,
                        &mut state.camera,
                        &state.map,
                        &state.game.objects,
                        &mut state.game.rng,
                        state.game.elapsed_ms as i32,
                        state.dialog.is_some(),
                    );
                } else {
                    state.fly_cam = None;
                    state.game.objects.pending_war_pairs.clear();
                    state.camera.takt(dt * 1000.0); // camera update (ms)
                }

                // Progress bars — panel clones (build stack / building
                // HP) + the floating HP bars over objects. Runs AFTER
                // the camera takt so the world→screen projection uses
                // the same camera state the frame renders with.
                refresh_progress_bars(state);

                state.minimap.takt(dt * 1000.0);
                // Water/object animation freezes while paused (the C++
                // pause return precedes CMatrixMap::Takt); a 0 ms takt
                // still uploads current state for rendering.
                let anim_ms = if state.is_paused { 0.0 } else { dt * 1000.0 };
                state.terrain.takt(
                    anim_ms,
                    &state.map,
                    &state.point_lights,
                    &state.camera,
                    &state.gfx.device,
                    &state.gfx.queue,
                ); // water animation + dynamic object tint updates

                // Push BASE per-sub-unit animation (platform rise,
                // door slide) from the live `Building::base_floor_progress`
                // into the BuildingsRenderer instance buffers. Ports
                // MatrixObjectBuilding.cpp:836-852. Must run AFTER
                // `state.game.takt` advances the base's state machine.
                state.terrain.sync_building_animation(
                    &state.gfx.queue,
                    &state.game.objects,
                    &state.map,
                    &state.point_lights,
                );

                // Per-frame turret-placement preview update — port of
                // `CMatrixSideUnit::LogicTakt`'s BUILDING_TURRET branch
                // (MatrixSide.cpp:528-585). Snaps the cursor to the
                // nearest free turret slot on the parent base, smooth-
                // rotates the ghost cannon to the slot's rest angle,
                // and flips the validity tint. Also collects free-slot
                // markers (port of `CreatePlacesShow`,
                // MatrixObjectBuilding.cpp:1617) — visible whenever the
                // picker is open so the player can see where to click.
                let (ghost, markers) = update_turret_build(state, step_ms);

                // Rebuild chassis instance buffers from the live arena
                // so newly-spawned robots show up (stand-in for
                // `CMatrixRobotAI::RNeed`'s per-robot matrix update —
                // MatrixObjectRobot.cpp:359-480).
                let t_sr = crate::platform::now_secs();
                {
                    // The sync runs after the logic MapScope closed, but
                    // do_chassis_animation / link_pneumatic / DIP tumble
                    // read `current_elapsed_ms()`. Without a scope that
                    // clock reads 0, the Move-anim cursor never advances,
                    // and pneumatic walkers — whose movement is foot-
                    // anchored to anim frames — freeze on the base pad
                    // and get killed there (robot_stuck.log repro).
                    let _scope = crate::matrix_game::map::MapScope::enter(
                        &state.map,
                        state.game.elapsed_ms,
                    );
                    state.terrain.sync_robots(
                        &state.gfx.device,
                        &state.gfx.queue,
                        &mut state.game.objects,
                        &state.map,
                        &state.point_lights,
                        // Chassis walk-anim cursors freeze while paused.
                        if state.is_paused { 0 } else { step_ms },
                        ghost,
                        &markers,
                        &state.camera,
                    );
                }
                state.perf_acc[3] += crate::platform::now_secs() - t_sr;

                // Bake the minimap background the first time — ports
                // CMinimap::RenderBackground (called once at map load). Must
                // be its own submission so `queue.write_buffer` doesn't get
                // clobbered by the main render's VP write.
                state.minimap.bake_background(
                    &state.gfx.device,
                    &state.gfx.queue,
                    &mut state.terrain,
                    &state.map,
                );
                let t_sync_end = crate::platform::now_secs();
                // sync_other = everything between logic end and frame
                // begin, minus the robot sync measured above.
                state.perf_acc[4] += t_sync_end - t_logic_end;

                match state.gfx.begin_frame() {
                    Ok((output, view, mut encoder)) => {
                        let vp = state.camera.view_proj();
                        let vm = state.camera.view_matrix();
                        state.terrain.render(
                            &state.gfx.device,
                            &mut encoder,
                            &view,
                            &state.gfx.queue,
                            &state.camera,
                            vp,
                            vm,
                            &state.map,
                        );
                        {
                            // Selection-ring pass — share the terrain's
                            // depth buffer so the dots are occluded by
                            // buildings / objects that sit in front of
                            // them. Upload first with the current view
                            // basis so the billboards face the camera.
                            let cr = state.camera.camera_right_world();
                            let cu = state.camera.camera_up_world();
                            let mc = glam::Vec2::new(
                                state.map.world_width() * 0.5,
                                state.map.world_height() * 0.5,
                            );
                            state
                                .selection_ring
                                .upload(&state.gfx.queue, vp, cr, cu, mc);
                            // ── Effect primitives (DrawEffects port) ──
                            state.bb_queue.clear();
                            // Robot on-mesh emissive lights (DrawLights).
                            if let Some(r) = state.terrain.robots() {
                                r.emit_light_billboards(&mut state.bb_queue);
                            }
                            // Move-order pings — billboards with the
                            // real `moveto` texture (SPointMoveTo::Draw).
                            state.move_to.draw(&state.map, &mut state.bb_queue);
                            state.mesh_queue.draws.clear();
                            for e in &state.game.effects {
                                e.draw(&mut state.bb_queue, &mut state.mesh_queue);
                            }
                            {
                                let objs = &state.game.objects;
                                let rng = &mut state.game.rng;
                                // Weapon draw reads current_elapsed_ms()
                                // (plasma muzzle-cone age, volcano flash
                                // angle) — the logic scope closed above,
                                // and with the clock at 0 the cone's age
                                // goes negative, never expires, and sticks
                                // to the barrel as a permanent glow.
                                let _scope = crate::matrix_game::map::MapScope::enter(
                                    &state.map,
                                    state.game.elapsed_ms,
                                );
                                for w in objs.weapons.iter() {
                                    w.draw(&mut state.bb_queue, objs, rng, state.is_paused);
                                }
                                // DIP wreck smoke (the pieces are the
                                // robot's own part meshes, drawn by the
                                // robots renderer).
                                for id in objs.iter_units() {
                                    let Some(o) = objs.get(id) else { continue };
                                    match o.core().obj_type {
                                        crate::matrix_game::map_static::ObjectType::RobotAi => {
                                            let r: &crate::matrix_game::robot::Robot = unsafe {
                                                &*(o as *const dyn crate::matrix_game::map_static::MapStatic
                                                    as *const crate::matrix_game::robot::Robot)
                                            };
                                            if r.state == crate::matrix_game::robot::RobotState::Dip
                                            {
                                                r.draw_dip(&mut state.bb_queue);
                                            } else if r.chassis
                                                == crate::matrix_game::robot::ChassisKind::AntiGravity
                                            {
                                                // CMatrixEffectFireStream — the antigrav jet
                                                // flames. Created per chassis at UnitInsert
                                                // (MatrixObjectRobot.cpp:91-95), repositioned
                                                // from chassis bones 10/11 each RNeed
                                                // (:578-599), and drawn as ONE of the five
                                                // fire-stream line billboards per frame
                                                // (MatrixEffectSmokeAndFire.cpp:406-425;
                                                // paused → fixed index 2).
                                                use crate::matrix_lib::three_g::billboard::{
                                                    TexRef, BBT_FIRESTREAM1,
                                                };
                                                let kidx = r.chassis.kind_index();
                                                if let Some(vo) =
                                                    crate::matrix_lib::three_g::vector_object::chassis_vo(kidx)
                                                {
                                                    let world =
                                                        crate::matrix_game::object_robot::robot_world_uncentered(r);
                                                    let frame = r.chassis_anim.vo_frame;
                                                    for mid in [10u32, 11u32] {
                                                        // Each jet is its own FireStream
                                                        // effect in the C++ — independent
                                                        // IRND(5) rolls per side.
                                                        let idx = if state.is_paused {
                                                            2
                                                        } else {
                                                            rng.range(0, 4) as usize
                                                        };
                                                        let Some(m) = vo
                                                            .matrix_by_id(mid, frame)
                                                            .or_else(|| vo.matrix_by_id(mid, 0))
                                                        else {
                                                            continue;
                                                        };
                                                        let bp = glam::Vec3::new(m[12], m[13], m[14]);
                                                        let by = glam::Vec3::new(m[4], m[5], m[6]);
                                                        let pos = world.transform_point3(bp);
                                                        let dir =
                                                            world.transform_vector3(-by).normalize_or_zero();
                                                        // m_StreamLen = 10, FIRE_STREAM_WIDTH = 5.
                                                        state.bb_queue.line(
                                                            pos,
                                                            pos + dir * 10.0,
                                                            5.0,
                                                            0xFFFF_FFFF,
                                                            TexRef::Bbt(BBT_FIRESTREAM1 + idx),
                                                        );
                                                    }
                                                }
                                            }
                                            // Repair-gun glow — line between weapon
                                            // bones 1/2 + end sprites, flickering
                                            // alpha, nudged 3 toward the camera
                                            // (SWeaponRepairData::Draw,
                                            // MatrixRobot.cpp:24-61).
                                            if r.state != crate::matrix_game::robot::RobotState::Dip
                                            {
                                                use crate::matrix_lib::three_g::billboard::{
                                                    TexRef, BBT_REPGLOW, BBT_REPGLOWEND,
                                                };
                                                let cam = state.camera.eye_pos_world();
                                                for (g0, g1) in r.repair_glows.iter().flatten() {
                                                    let a: u32 = if state.is_paused {
                                                        240
                                                    } else {
                                                        128 + rng.range(0, 127) as u32
                                                    };
                                                    let color = (a << 24) | 0x00FF_FFFF;
                                                    let p0 = *g0
                                                        + (cam - *g0).normalize_or_zero() * 3.0;
                                                    let p1 = *g1
                                                        + (cam - *g1).normalize_or_zero() * 3.0;
                                                    state.bb_queue.line(
                                                        p0,
                                                        p1,
                                                        10.0,
                                                        color,
                                                        TexRef::Bbt(BBT_REPGLOW),
                                                    );
                                                    state.bb_queue.billboard(
                                                        p0,
                                                        5.0,
                                                        0.0,
                                                        color,
                                                        TexRef::Bbt(BBT_REPGLOWEND),
                                                    );
                                                    state.bb_queue.billboard(
                                                        p1,
                                                        10.0,
                                                        0.0,
                                                        color,
                                                        TexRef::Bbt(BBT_REPGLOWEND),
                                                    );
                                                }
                                            }
                                        }
                                        crate::matrix_game::map_static::ObjectType::Cannon => {
                                            let c: &crate::matrix_game::object_cannon::Cannon = unsafe {
                                                &*(o as *const dyn crate::matrix_game::map_static::MapStatic
                                                    as *const crate::matrix_game::object_cannon::Cannon)
                                            };
                                            if c.state == crate::matrix_game::object_cannon::CannonState::Dip {
                                                for u in &c.dip_units {
                                                    u.smoke.draw(&mut state.bb_queue);
                                                }
                                            }
                                        }
                                        crate::matrix_game::map_static::ObjectType::Building => {
                                            let b: &crate::matrix_game::object_building::Building = unsafe {
                                                &*(o as *const dyn crate::matrix_game::map_static::MapStatic
                                                    as *const crate::matrix_game::object_building::Building)
                                            };
                                            // Capture-progress ring
                                            // (CMatrixEffectZahvat::Draw).
                                            if let Some(z) = b.zahvat.as_ref() {
                                                z.draw(&mut state.bb_queue);
                                            }
                                        }
                                        crate::matrix_game::map_static::ObjectType::Flyer => {
                                            let f: &crate::matrix_game::flyer::Flyer = unsafe {
                                                &*(o as *const dyn crate::matrix_game::map_static::MapStatic
                                                    as *const crate::matrix_game::flyer::Flyer)
                                            };
                                            // Tractor-beam spirals
                                            // (CMatrixEffectElevatorField::Draw).
                                            if let Some(e) = f.carry.elevator.as_ref() {
                                                e.draw(&mut state.bb_queue);
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            let inv_view = vm.inverse();
                            let cam_pos = glam::Vec3::new(
                                inv_view.w_axis.x + mc.x,
                                inv_view.w_axis.y + mc.y,
                                inv_view.w_axis.z,
                            );
                            state.effects_renderer.upload(
                                &state.gfx.device,
                                &state.gfx.queue,
                                &state.bb_queue,
                                vp,
                                vm,
                                cam_pos,
                                cr,
                                cu,
                                mc,
                            );
                            state.effects_renderer.upload_spots(
                                &state.gfx.device,
                                &state.gfx.queue,
                                &state.spots,
                                mc,
                            );
                            state
                                .effect_meshes
                                .upload(&state.gfx.queue, &state.mesh_queue, vp, mc);
                            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("Selection Ring Pass"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: &view,
                                    resolve_target: None,
                                    depth_slice: None,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Load,
                                        store: wgpu::StoreOp::Store,
                                    },
                                })],
                                depth_stencil_attachment: Some(
                                    wgpu::RenderPassDepthStencilAttachment {
                                        view: state.terrain.depth_view(),
                                        depth_ops: Some(wgpu::Operations {
                                            load: wgpu::LoadOp::Load,
                                            store: wgpu::StoreOp::Store,
                                        }),
                                        stencil_ops: Some(wgpu::Operations {
                                            load: wgpu::LoadOp::Load,
                                            store: wgpu::StoreOp::Store,
                                        }),
                                    },
                                ),
                                timestamp_writes: None,
                                occlusion_query_set: None,
                                multiview_mask: None,
                            });
                            // Landscape decals first (the C++ draws
                            // them right after the terrain surfaces).
                            state.effects_renderer.render_spots(&mut pass);
                            // Projectile / debris meshes (depth-write).
                            state.effect_meshes.render(&mut pass);
                            state.selection_ring.render(&mut pass);
                            // Effect billboards (alpha bucket sorted,
                            // additive after) — `DrawEffects`.
                            state.effects_renderer.render(&mut pass);
                            // Marquee shares the overlay color target +
                            // depth attachment with the selection ring;
                            // render inline to avoid a separate pass.
                            state.marquee.render(&mut pass);
                        }
                        {
                            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("Minimap Pass"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: &view,
                                    resolve_target: None,
                                    depth_slice: None,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Load,
                                        store: wgpu::StoreOp::Store,
                                    },
                                })],
                                depth_stencil_attachment: None,
                                timestamp_writes: None,
                                occlusion_query_set: None,
                                multiview_mask: None,
                            });
                            state.minimap.render(
                                &state.gfx.queue,
                                &mut pass,
                                state.gfx.config.width as f32,
                                state.gfx.config.height as f32,
                                &state.map,
                                &state.camera,
                                &state.game.objects,
                            );
                        }
                        // Pause overlay — fullscreen translucent black
                        // tint when the game is paused. Sits between
                        // world+minimap and the UI so the HUD stays
                        // bright while the playfield dims. Port of
                        // MatrixMap.cpp:2430-2454.
                        let pause_visible = state.is_paused
                            || state
                                .game
                                .player_side
                                .builder
                                .as_ref()
                                .map(|b| b.active)
                                .unwrap_or(false);
                        if pause_visible {
                            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("Pause Overlay Pass"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: &view,
                                    resolve_target: None,
                                    depth_slice: None,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Load,
                                        store: wgpu::StoreOp::Store,
                                    },
                                })],
                                depth_stencil_attachment: None,
                                timestamp_writes: None,
                                occlusion_query_set: None,
                                multiview_mask: None,
                            });
                            state.pause_overlay.draw(&mut pass);
                        }
                        // Interface (HUD) pass — draws on top of
                        // world + minimap. Ports
                        // `CIFaceList::Render` iteration.
                        {
                            // Pause hint — port of MatrixLogic.cpp:2607-2614:
                            //   m_PauseHint = CMatrixHint::Build(TEMPLATE_PAUSE);
                            //   m_PauseHint->Show(14, 62);
                            // Built each frame while paused (cheap layout).
                            // Falls through to the regular hover hint when
                            // unpaused or when the "Pause" template is
                            // missing from `da/Templates`. Build BEFORE
                            // borrowing `iface_list.panels` because the
                            // builder needs `&mut iface_renderer`.
                            // No pause hint while a dialog is up — the C++
                            // gate at MatrixLogic.cpp:2609 includes
                            // `!FLAG(m_Flags, MMFLAG_DIALOG_MODE)`.
                            let pause_hint_local: Option<crate::matrix_game::interface::Hint> =
                                if pause_visible && state.dialog.is_none() {
                                    build_pause_hint(state)
                                } else {
                                    None
                                };
                            let panels: Vec<&crate::matrix_game::interface::CInterface> =
                                state.iface_list.panels.iter().collect();
                            let hint_to_show = pause_hint_local
                                .as_ref()
                                .or_else(|| state.iface_list.hint_system.active());
                            // Dialog-mode widgets (Menu / Win / Stat /
                            // confirmation boxes) — drawn under the
                            // Hints button panel, see the renderer's
                            // two-pass comment.
                            let dialog_hints: Vec<&crate::matrix_game::interface::Hint> = state
                                .dialog
                                .as_ref()
                                .map(|d| d.hints.iter().collect())
                                .unwrap_or_default();
                            // Software-cursor quad — scaled with the
                            // rest of the UI (the original draws raw
                            // 32 px at its 1024×768 design res).
                            let cursor_sprite = {
                                let scale = (state.gfx.config.height as f32
                                    / crate::matrix_game::interface::interface::DESIGN_H)
                                    .max(0.1);
                                state
                                    .matrix_cursor
                                    .tex_path()
                                    .and_then(|p| state.iface_renderer.atlas_size(p))
                                    .and_then(|(tw, th)| {
                                        state.matrix_cursor.sprite(tw, th, scale)
                                    })
                            };
                            state.iface_renderer.set_cursor_sprite(cursor_sprite);
                            state.iface_renderer.upload_with_popup_and_hint(
                                &state.gfx.device,
                                &state.gfx.queue,
                                &panels,
                                state.iface_list.popup.as_ref(),
                                hint_to_show,
                                &dialog_hints,
                                Some(&state.iface_list.hint_chrome),
                                state.gfx.config.width as f32,
                                state.gfx.config.height as f32,
                            );
                            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("Interface Pass"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: &view,
                                    resolve_target: None,
                                    depth_slice: None,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Load,
                                        store: wgpu::StoreOp::Store,
                                    },
                                })],
                                depth_stencil_attachment: None,
                                timestamp_writes: None,
                                occlusion_query_set: None,
                                multiview_mask: None,
                            });
                            // Panels only — the popup / hints /
                            // dialogs render after the constructor
                            // preview (the C++ z-order draws
                            // CConstructor::Render mid-interface,
                            // CInterface.cpp:929-933).
                            state.iface_renderer.render_base(&mut pass);
                        }
                        // Constructor 3D preview — one chassis drawn
                        // into the sub-viewport on the Base panel.
                        // Ports CConstructor::Render's viewport setup
                        // (CConstructor.cpp:264-360). Runs after the
                        // panel pass so the backdrop is in place, but
                        // below popups/hints (render_overlay below).
                        let q_opt = builder_preview_query(state);
                        let robots_ok = state.terrain.robots().is_some();
                        {
                            static LOGGED: std::sync::atomic::AtomicBool =
                                std::sync::atomic::AtomicBool::new(false);
                            static LAST_STATE: std::sync::atomic::AtomicU8 =
                                std::sync::atomic::AtomicU8::new(0xFF);
                            let s = (q_opt.is_some() as u8) | ((robots_ok as u8) << 1);
                            if LAST_STATE.swap(s, std::sync::atomic::Ordering::Relaxed) != s
                                || !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed)
                            {
                                log::debug!(
                                    "preview: query={} robots_renderer={}",
                                    if q_opt.is_some() { "Some" } else { "None" },
                                    if robots_ok { "Some" } else { "None" },
                                );
                            }
                        }
                        if let Some(q) = q_opt {
                            let chassis = q.chassis;
                            let angle_rad = q.angle_rad;
                            let design_rect = q.design_rect;
                            let armor_kind = q.armor_kind;
                            let head_kind = q.head_kind;
                            let weapon_kinds = q.weapon_kinds;
                            if let Some(robots) = state.terrain.robots() {
                                // Design-space → pixel rect: the Base
                                // panel resolves its top-left via
                                // CInterface::resolved_pos; design
                                // coords are Y-down from that origin.
                                let surface_w = state.gfx.config.width;
                                let surface_h = state.gfx.config.height;
                                let scale = (surface_h as f32
                                    / crate::matrix_game::interface::interface::DESIGN_H)
                                    .max(0.1);
                                let panel = state
                                    .iface_list
                                    .panel("Base")
                                    .map(|p| {
                                        p.resolved_pos(surface_w as f32, surface_h as f32, scale)
                                    })
                                    .unwrap_or([0.0, 0.0]);
                                let sx = (panel[0] + design_rect[0] * scale).max(0.0) as u32;
                                let sy = (panel[1] + design_rect[1] * scale).max(0.0) as u32;
                                let sw = (design_rect[2] * scale) as u32;
                                let sh = (design_rect[3] * scale) as u32;

                                let depth_view = state.terrain.depth_view();
                                let mut pass =
                                    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                        label: Some("Constructor Preview Pass"),
                                        color_attachments: &[Some(
                                            wgpu::RenderPassColorAttachment {
                                                view: &view,
                                                resolve_target: None,
                                                depth_slice: None,
                                                ops: wgpu::Operations {
                                                    load: wgpu::LoadOp::Load,
                                                    store: wgpu::StoreOp::Store,
                                                },
                                            },
                                        )],
                                        depth_stencil_attachment: Some(
                                            wgpu::RenderPassDepthStencilAttachment {
                                                view: depth_view,
                                                depth_ops: Some(wgpu::Operations {
                                                    // Clear just the preview
                                                    // region via scissor — but
                                                    // we can't scissor clears,
                                                    // so load the existing
                                                    // depth. The robot draw
                                                    // will depth-test against
                                                    // whatever was there;
                                                    // since the preview region
                                                    // is over UI (sky depth),
                                                    // the chassis effectively
                                                    // writes freely.
                                                    load: wgpu::LoadOp::Clear(1.0),
                                                    store: wgpu::StoreOp::Store,
                                                }),
                                                stencil_ops: Some(wgpu::Operations {
                                                    load: wgpu::LoadOp::Load,
                                                    store: wgpu::StoreOp::Store,
                                                }),
                                            },
                                        ),
                                        timestamp_writes: None,
                                        occlusion_query_set: None,
                                        multiview_mask: None,
                                    });
                                robots.render_preview_full(
                                    &state.gfx.queue,
                                    &mut pass,
                                    chassis,
                                    armor_kind,
                                    head_kind,
                                    &weapon_kinds,
                                    angle_rad,
                                    [sx, sy, sw, sh],
                                    surface_w,
                                    surface_h,
                                    None,
                                    Some(&state.effects_renderer),
                                );
                            }
                        }
                        // Popup / hint / dialog overlay — above the
                        // constructor preview.
                        {
                            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("Interface Overlay Pass"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: &view,
                                    resolve_target: None,
                                    depth_slice: None,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Load,
                                        store: wgpu::StoreOp::Store,
                                    },
                                })],
                                depth_stencil_attachment: None,
                                timestamp_writes: None,
                                occlusion_query_set: None,
                                multiview_mask: None,
                            });
                            state.iface_renderer.render_overlay(&mut pass);
                        }
                        // Progress-bar overlay pass — on top of the UI.
                        {
                            state.progress_bars.upload(
                                &state.gfx.device,
                                &state.gfx.queue,
                                state.gfx.config.width as f32,
                                state.gfx.config.height as f32,
                                Some(&state.camera),
                            );
                            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("ProgressBar Pass"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: &view,
                                    resolve_target: None,
                                    depth_slice: None,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Load,
                                        store: wgpu::StoreOp::Store,
                                    },
                                })],
                                depth_stencil_attachment: None,
                                timestamp_writes: None,
                                occlusion_query_set: None,
                                multiview_mask: None,
                            });
                            state.progress_bars.render(&mut pass);
                            // Software cursor — the C++ frame's very
                            // last draw (MatrixMap.cpp:2485).
                            state.iface_renderer.render_cursor(&mut pass);
                        }
                        state.gfx.end_frame(output, encoder);
                        state.perf_acc[5] += crate::platform::now_secs() - t_sync_end;
                    }
                    Err(wgpu::SurfaceError::Lost) => {
                        let size = state.window.inner_size();
                        state.gfx.resize(size.width, size.height);
                    }
                    Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                    Err(e) => log::warn!("surface error: {e:?}"),
                }

                state.window.request_redraw();
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = self.state.borrow().as_ref() {
            state.window.request_redraw();
        }
    }
}

// Load map — native reads from pkg, returns map + map storage + global
// matrix data storage (robots.dat) + pkg for texture loading.
//
// Matches the original startup contract (MatrixGame.cpp:240-257, 383-399):
// the caller can request a specific map via the first CLI argument; if
// absent, we fall back to `Config/Map` in the global data. Global data
// itself is a required dependency (MatrixGame.cpp:240-257) — the engine
// dereferences `g_MatrixData->BlockGet(...)` without NULL guards all over
// Init, so missing `robots.dat` is a fatal startup error. See `load_map`
// (native) and `load_map_async` (wasm) further below.

/// Port of `CMatrixSideUnit::PlayerAction` + the follow-on
/// `CBuildStack::AddItem` call. Dispatches the button identified by
/// its `Name` to the right game-state change — e.g. `buro` (build
/// robot) opens the `RobotBuilder` constructor panel like the C++
/// `m_ConstructPanel` (CConstructor.cpp:970-975).
fn dispatch_ui_click(state: &mut AppState, click: &crate::matrix_game::interface::Click) {
    use crate::matrix_game::config::RobotUnitKind;
    use crate::matrix_game::interface::constructor::parse_constructor_button;
    use crate::matrix_game::interface::iface_list::TurretKind;
    use crate::matrix_game::interface::Click;
    use crate::matrix_game::map_static::{MapStatic, ObjectType};
    use crate::matrix_game::object_building::{Building, BuildingType};
    use crate::matrix_game::object_robot::RobotUnitType;
    use crate::matrix_game::robot::ChassisKind;
    use crate::matrix_game::side::CurrSel;

    let name = match click {
        Click::Button(n) => n.as_str(),
        // Right-button clicks are dispatched separately
        // (see `dispatch_ui_right_click`). Returning early here keeps
        // the LMB handler's match arms uncluttered.
        Click::RightButton(_) => return,
        // Popup-menu item commit. Port of `CIFaceMenu::OnMenuItemPress`
        // (CIFaceMenu.cpp:530+) — calls SuperDjeans with the chosen
        // (type, kind, pilon) then closes the popup.
        Click::PopupItem { parent, kind } => {
            use crate::matrix_game::interface::iface_menu::EMenuParent;
            let ty = parent.unit_type();
            let pilon = parent.pilon();
            // Pylon-empty kind (RUK_UNKNOWN) is a valid selection for
            // `heade` / `weape` ("clear this slot"); pass through.
            if let Some(b) = state.game.player_side.builder.as_mut() {
                b.super_djeans(ty, *kind, pilon, false);
                log::debug!(
                    "popup commit: type={:?} kind={} pilon={} cfg.chassis={} hull={} weap0={}",
                    ty,
                    kind.0,
                    pilon,
                    b.cfg().chassis.kind.0,
                    b.cfg().hull.unit.kind.0,
                    b.cfg().weapon[0].kind.0,
                );
            }
            reset_build_counter(state);
            state.iface_list.popup = None;
            state.iface_list.popup_restore_pending = None;
            state.iface_list.popup_focus_clear_pending = true;
            let _ = EMenuParent::PylonChassis; // keep import live
            return;
        }
    };

    // ── Static `ON_PRESS` handlers (CInterface.cpp:577-586) ───────
    // In the C++, `basepl` / `titpl` / `elecpl` / `energpl` / `plaspl`
    // are CIFaceStatic elements bound to `CIFaceList::JumpToBuilding`
    // (CInterface.cpp:4552-4559). The callback centres the strategy
    // camera on the currently-selected building's geo center.
    // JumpToBuilding is wired to ALL five plant portraits
    // (titpl/plaspl/elecpl/batpl/basepl, CInterface.cpp:577-586).
    if matches!(name, "basepl" | "titpl" | "plaspl" | "elecpl" | "batpl") {
        if matches!(
            state.game.player_side.curr_sel,
            CurrSel::BuildingSelected | CurrSel::BaseSelected
        ) {
            if let Some(id) = state.game.active_object() {
                if let Some(obj) = state.game.objects.get(id) {
                    let p = obj.core().geo_center;
                    state.camera.set_xy_strategy([p.x, p.y]);
                    log::debug!(
                        "{name}: center camera on building at ({:.1},{:.1})",
                        p.x,
                        p.y
                    );
                }
            }
        }
        return;
    }

    // ── Top-level menu buttons ────────────────────────────────────
    match name {
        // Arcade-mode buttons (CInterface.cpp:3418-3421, 3480-3483):
        // enter robot / leave robot / self-destruct.
        "inro" => {
            enter_robot(state);
            return;
        }
        "lero" => {
            leave_robot(state);
            return;
        }
        "sbo" => {
            if state.game.is_arcade_mode() {
                if let Some(id) = state.game.objects.arcaded_object {
                    state.game.robot_big_boom(&state.map, id);
                }
            }
            return;
        }
        "buro" => {
            // Port of MatrixFormGame.cpp:1385-1389 + CConstructor.cpp:
            // 970-975 — reset the build-multiplier counter, validate it
            // against the side's resources, then open the constructor.
            if state.game.player_side.curr_sel != CurrSel::BaseSelected {
                log::debug!("buro: no base selected, ignoring");
                return;
            }
            state.iface_list.r_count_control.reset();
            let ctx = build_counter_ctx(state);
            state.iface_list.r_count_control.check_up(ctx);
            if let Some(b) = state.game.player_side.builder.as_mut() {
                b.activate();
            }
            // Pause the game while the constructor is open — port of
            // `g_MatrixMap->Pause(true)` at CConstructor.cpp:975.
            // Pause(true) also silences everything (MatrixMap.hpp:638).
            state.is_paused = true;
            state.sounds.stop_all();
            log::debug!("buro: opened robot constructor");
            // Silence "unused import" on paths we only need in other
            // arms of this match.
            let _ = (BuildingType::Base, ChassisKind::Track);
            let _: Option<&dyn MapStatic> = None;
            let _ = ObjectType::Building;
            let _: fn(&mut Building) = |_b: &mut Building| {};
            return;
        }
        "buca" => {
            // Port of CInterface.cpp:3493-3499 — IF_BUILD_CA opens the
            // turret-kind picker on the Main panel. The C++ flips
            // ORDERING_MODE + PREORDER_BUILD_TURRET; the actual kind is
            // not picked yet — that comes when the user clicks one of
            // the `tur1..tur4` buttons. Visibility for any selected
            // building (base OR factory) since `buca` is shown for
            // BUILDING_SELECTED / BASE_SELECTED at CInterface.cpp:1600.
            if !matches!(
                state.game.player_side.curr_sel,
                CurrSel::BaseSelected | CurrSel::BuildingSelected
            ) {
                log::debug!("buca: no building selected, ignoring");
                return;
            }
            let Some(id) = state.game.active_object() else {
                return;
            };
            // Open the kind picker only — the placement-preview ghost
            // is held off until the player commits a kind via tur1..4.
            // Mirrors the C++ at CInterface.cpp:3493-3499 where the
            // PREORDER_BUILD_TURRET flag flips before BeginBuildTurret
            // (and thus before `m_CannonForBuild.m_Cannon` is created).
            state.iface_list.turret_build.open_picker(id);
            log::debug!("buca: entered turret-build mode (kind picker)");
            return;
        }
        "cocan" => {
            // Port of CConstructorPanel::ResetGroupNClose
            // (CConstructor.cpp:986-994). Despite the name, this only
            // clears `m_Active` + closes the popup — it does NOT reset
            // the live construction state. The next `buro` reopens with
            // the same chassis/armor/head/weapons the user had.
            if let Some(b) = state.game.player_side.builder.as_mut() {
                b.deactivate();
            }
            // Resume the game — port of `g_MatrixMap->Pause(false)` at
            // CConstructor.cpp:992.
            state.is_paused = false;
            state.iface_list.r_count_control.reset();
            state.iface_list.popup = None;
            state.iface_list.popup_restore_pending = None;
            state.iface_list.turret_build.cancel();
            log::debug!("cocan: constructor closed");
            return;
        }
        "cobuild" => {
            commit_and_queue_robot(state);
            return;
        }
        "hisleft" => {
            if let Some(cfg) = state.iface_list.history.prev() {
                if let Some(b) = state.game.player_side.builder.as_mut() {
                    b.apply_config(cfg);
                    log::debug!(
                        "hisleft: loaded preset cursor={}",
                        state.iface_list.history.cursor
                    );
                }
                reset_build_counter(state);
            }
            return;
        }
        "hisright" => {
            if let Some(cfg) = state.iface_list.history.next() {
                if let Some(b) = state.game.player_side.builder.as_mut() {
                    b.apply_config(cfg);
                    log::debug!(
                        "hisright: loaded preset cursor={}",
                        state.iface_list.history.cursor
                    );
                }
                reset_build_counter(state);
            }
            return;
        }
        "bup" => {
            let ctx = build_counter_ctx(state);
            state.iface_list.r_count_control.up(ctx);
            return;
        }
        "bdown" => {
            let ctx = build_counter_ctx(state);
            state.iface_list.r_count_control.down(ctx);
            return;
        }
        "tur1" | "tur2" | "tur3" | "tur4" => {
            // Turret kind picker — names match the `tur{N}` buttons on
            // the Main panel (StringConstants.hpp IF_MAIN_TURRET*).
            let n: i32 = name.trim_start_matches("tur").parse().unwrap_or(1);
            let Some(kind) = TurretKind::from_i32(n) else {
                return;
            };
            let parent = state.iface_list.turret_build.parent.or_else(|| {
                if state.game.player_side.curr_sel == CurrSel::BaseSelected {
                    state.game.active_object()
                } else {
                    None
                }
            });
            if let Some(parent) = parent {
                state.iface_list.turret_build.begin(kind, parent);
                log::debug!("tur{}: selected kind={:?}", n, kind);
            }
            return;
        }
        "ocan" => {
            // Port of CInterface.cpp:3465-3470 — IF_ORDER_CANCEL clears
            // any in-flight turret placement (`m_CannonForBuild.Delete`
            // + `m_CurrentAction = NOTHING_SPECIAL`) and resets the
            // ordering mode, robot pre-orders included.
            state.iface_list.turret_build.cancel();
            state.iface_list.pre_order = None;
            log::debug!("ocan: cancelled ordering mode");
            return;
        }
        // Robot order buttons (IF_ORDER_*, CInterface.cpp:3447-3479).
        // `ofi` / `ogo` arm a pre-order executed by the next map click;
        // `ost` acts immediately.
        "ofi" => {
            state.iface_list.pre_order =
                Some(crate::matrix_game::interface::iface_list::PreOrder::Fire);
            log::debug!("order: attack armed — click a target or the ground");
            return;
        }
        "ogo" => {
            state.iface_list.pre_order =
                Some(crate::matrix_game::interface::iface_list::PreOrder::Move);
            log::debug!("order: move armed — click a destination");
            return;
        }
        "ost" => {
            state.iface_list.pre_order = None;
            state.game.order_stop(&state.map);
            log::debug!("order: stop issued");
            return;
        }
        "oca" => {
            state.iface_list.pre_order =
                Some(crate::matrix_game::interface::iface_list::PreOrder::Capture);
            log::debug!("order: capture armed — click an enemy building");
            return;
        }
        "opa" => {
            state.iface_list.pre_order =
                Some(crate::matrix_game::interface::iface_list::PreOrder::Patrol);
            log::debug!("order: patrol armed — click a destination");
            return;
        }
        "obomb" => {
            state.iface_list.pre_order =
                Some(crate::matrix_game::interface::iface_list::PreOrder::Bomb);
            log::debug!("order: bomb armed — click a target");
            return;
        }
        "orep" => {
            state.iface_list.pre_order =
                Some(crate::matrix_game::interface::iface_list::PreOrder::Repair);
            log::debug!("order: repair armed — click a damaged friendly");
            return;
        }
        // Auto-order toggles act immediately (CInterface.cpp:3480-3499).
        // Auto-order toggles: the `_ON` variant (shown while active)
        // cancels via PGOrderStop; the `_OFF` variant arms the auto order
        // (CInterface.cpp:3409-3445). The `AUTO_*_ON` flag tracks the state
        // and drives which variant is shown.
        "oacapn" | "oacapf" => {
            let no = state.game.sel_group_to_logic_group();
            if name == "oacapn" {
                state.game.pg_order_stop(&state.map, no);
            } else {
                state.game.pg_order_auto_capture(&state.map, no);
            }
            return;
        }
        "oafrn" | "oafrf" | "oafron" | "oafrof" => {
            let no = state.game.sel_group_to_logic_group();
            if name == "oafrn" || name == "oafron" {
                state.game.pg_order_stop(&state.map, no);
            } else {
                state.game.pg_order_auto_attack(&state.map, no);
            }
            return;
        }
        "oafcn" | "oafcf" => {
            let no = state.game.sel_group_to_logic_group();
            if name == "oafcn" {
                state.game.pg_order_stop(&state.map, no);
            } else {
                state.game.pg_order_auto_defence(&state.map, no);
            }
            return;
        }
        // IF_CALL_FROM_HELL — reinforcements / maintenance supply drop
        // (CInterface.cpp:3505-3508). `bure` (IF_BUILD_REPAIR) is a
        // separate build-repair placement pre-order, not modelled here.
        "callhell" => {
            if let Some(b) = state.game.active_object() {
                state.game.building_maintenance(&state.map, b);
                log::debug!("maintenance: requested");
            }
            return;
        }
        // IF_MAIN_MENU_BUTTON — open the in-game menu dialog
        // (CInterface.cpp:3407-3408).
        "mm" => {
            enter_dialog_mode(state, "Menu");
            return;
        }
        // IF_SHOWROBOTS_BUTT — cyan minimap ping on every player robot
        // (CMinimap::ShowPlayerBots, MatrixMinimap.cpp:1381-1390).
        "srb" => {
            let objs = &state.game.objects;
            let pings: Vec<[f32; 2]> = objs
                .iter_units()
                .filter_map(|id| objs.get(id))
                .filter(|o| {
                    matches!(
                        o.core().obj_type,
                        crate::matrix_game::map_static::ObjectType::RobotAi
                    ) && o.side() == crate::matrix_game::common::PLAYER_SIDE
                })
                .map(|o| [o.core().geo_center.x, o.core().geo_center.y])
                .collect();
            for [x, y] in pings {
                state.minimap.add_event(x, y, 0xFF00FFFF, 0xFF00FFFF);
            }
            return;
        }
        // Group panel icon (CInterface.cpp:1061-1080): LMB switches the
        // active member; LMB on the already-active icon promotes it to
        // a single-robot selection; Shift+LMB removes it from the group.
        n if n.starts_with("_dyngroup_") => {
            if let Ok(idx) = n["_dyngroup_".len()..].parse::<usize>() {
                let member = state
                    .game
                    .player_side
                    .cur_group()
                    .and_then(|g| g.get_object_by_n(idx as i32));
                if let Some(rid) = member {
                    if state.shift_down {
                        state.game.remove_object_from_selected_group(rid);
                        state
                            .game
                            .player_side
                            .selected
                            .retain(|&x| x != rid);
                        state.game.sync_group_from_selection();
                    } else if state.game.player_side.cur_sel_num == idx as i32 {
                        // Promote to single (CreateGroupFromCurrent +
                        // Select(ROBOT), CInterface.cpp:1070-1076).
                        state.game.player_side.select_single(
                            rid,
                            crate::matrix_game::side::CurrSel::RobotsSelected,
                        );
                        state.game.sync_group_from_selection();
                    } else {
                        state.game.player_side.cur_sel_num = idx as i32;
                    }
                }
            }
            return;
        }
        // Personal portrait → jump camera to the active robot
        // (CIFaceList::JumpToRobot, CInterface.cpp:4562-4570).
        n if n.starts_with("_dynpers_") => {
            let target = state
                .game
                .player_side
                .get_cur_sel_object()
                .or(state.game.player_side.active_object)
                .and_then(|id| state.game.objects.get(id))
                .map(|o| [o.core().geo_center.x, o.core().geo_center.y]);
            if let Some(pos) = target {
                state.camera.set_xy_strategy(pos);
            }
            return;
        }
        // Build-queue icon → cancel that queued item with refund
        // (CInterface.cpp:1081-1086 → CBuildStack::DeleteItem).
        n if n.starts_with("_dynstack_") => {
            if let Ok(no) = n["_dynstack_".len()..].parse::<usize>() {
                if let Some(bid) = state.game.active_object() {
                    if let Some(b) =
                        crate::matrix_game::logic::building_mut(&mut state.game.objects, bid)
                    {
                        if b.build_stack.delete_item(no) {
                            log::debug!("build stack: cancelled item {}", no);
                        }
                    }
                }
            }
            return;
        }
        _ => {}
    }

    // ── Constructor pylon buttons (LMB) ──────────────────────────
    // Port of the pylon ON_UN_PRESS wiring at CInterface.cpp:262 —
    // left-click fires CConstructor::RemoteOperateUnit, which cycles
    // the mounted component kind with wrap-around.
    if matches!(
        name,
        "pich" | "pihu" | "pihe" | "pi1" | "pi2" | "pi3" | "pi4" | "pi5"
    ) {
        if state.game.player_side.curr_sel != CurrSel::BaseSelected {
            return;
        }
        let active = state
            .game
            .player_side
            .builder
            .as_ref()
            .map(|b| b.active)
            .unwrap_or(false);
        if active {
            if let Some(b) = state.game.player_side.builder.as_mut() {
                b.remote_operate_unit(name);
            }
            reset_build_counter(state);
        }
        return;
    }

    // ── Direct component buttons (from the popup overlay) ─────────
    // chas1..5 / hull1..6 / head1..7 / weap1..10 — in the C++ these
    // are only clickable when CInterface opens a popup with the items
    // laid out at computed positions. The popup mechanic isn't ported
    // yet; as long as the data positions them stacked at the default
    // template slot the direct-click path can't reliably pick a
    // specific kind. We still wire the dispatch so that *if* a popup
    // mechanic lands later, the handler is already in place.
    if let Some((ty, kind, pilon)) = parse_constructor_button(name) {
        if state.game.player_side.curr_sel != CurrSel::BaseSelected {
            return;
        }
        if let Some(b) = state.game.player_side.builder.as_mut() {
            b.super_djeans(ty, kind, pilon, false);
            let p = b.construction_price();
            log::debug!(
                "constructor: {} {:?}/{} → titan={} elec={} energy={} plasma={} structure={}",
                name,
                ty,
                kind.0,
                p.titan(),
                p.electronics(),
                p.energy(),
                p.plasma(),
                b.construction_structure(),
            );
        }
        reset_build_counter(state);
        return;
    }

    log::debug!("dispatch_ui_click: unhandled click {:?}", click);
    let _ = RobotUnitType::Chassis;
    let _ = RobotUnitKind::UNKNOWN;
}

/// Dispatch a UI right-click. Currently routes pylon right-clicks to
/// the constructor popup menu (port of `CIFaceButton::OnMouseRBDown`
/// at CIFaceButton.cpp:183-321).
/// Execute the armed robot order at a world click — port of the
/// `PREORDER_FIRE` / `PREORDER_MOVE` branches of the side's
/// OnLButtonDown (MatrixSide.cpp:684-727). The attack order hits the
/// object under the cursor when there is one (any side — friendly
/// fire is the player's prerogative, like `PGOrderAttack(.., pObject)`),
/// otherwise the bare terrain point.
/// The SR2 host passes the real mission briefing via the `_begin` /
/// `_win` / `_loose` replaces (MatrixGame.cpp:274-308); the standalone
/// data ships developer placeholders ("Go! Go! Go!"). We load the same
/// Lang.dat RobotsMap texts from `assets/menu/maps.txt` (generated by
/// `tools/lang_dat.py --maps`); without them, compose a task
/// description from the map's actual win condition instead.
fn seed_mission_briefing(
    iface_list: &mut crate::matrix_game::interface::IFaceList,
    game: &crate::matrix_game::logic::MapLogic,
    texts: Option<&MapTexts>,
) {
    let fallback_begin = if game.before_win_count > 0 {
        "Задание: найдите и уничтожьте особые объекты противника."
    } else {
        "Задание: уничтожьте всех роботов противника\r\nили захватите все его базы."
    };
    let pick = |s: Option<&String>, fb: &str| -> String {
        s.filter(|s| !s.is_empty()).cloned().unwrap_or_else(|| fb.to_string())
    };
    let begin = pick(texts.map(|t| &t.begin), fallback_begin);
    let win = pick(texts.map(|t| &t.win), "Задание выполнено!");
    let loose = pick(texts.map(|t| &t.loose), "Задание провалено.");
    iface_list.hint_replacer.set("_begin", &begin);
    iface_list.hint_replacer.set("_win", &win);
    iface_list.hint_replacer.set("_loose", &loose);
}

/// Briefing texts handed over by the Rangers host — the `txt_start` /
/// `txt_win` / `txt_loss` arguments of the DLL `Run` (MatrixGameDll.cpp).
/// The host process passes them as UTF-8 files named by `MG_TXT_BEGIN` /
/// `MG_TXT_WIN` / `MG_TXT_LOSS`.
#[cfg(not(target_arch = "wasm32"))]
fn host_map_texts() -> Option<MapTexts> {
    let read = |var: &str| -> String {
        std::env::var(var)
            .ok()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default()
    };
    let begin = read("MG_TXT_BEGIN");
    (!begin.is_empty()).then(|| MapTexts {
        begin,
        win: read("MG_TXT_WIN"),
        loose: read("MG_TXT_LOSS"),
    })
}

/// Report the session outcome to the Rangers host: `g_ExitState` plus the
/// player-side statistics `Run` copies into `SRobotGameState`
/// (MatrixGameDll.cpp protectedBlock2). Written to the `MG_RESULT` file.
#[cfg(not(target_arch = "wasm32"))]
fn write_host_result(exit_state: i32, game: &crate::matrix_game::logic::MapLogic) {
    use crate::matrix_game::side::Stat;
    let Ok(path) = std::env::var("MG_RESULT") else {
        return;
    };
    let side = &game.player_side;
    let text = format!(
        "exit={}\ntime={}\nrobot_build={}\nrobot_kill={}\nturret_build={}\nturret_kill={}\nbuilding_kill={}\n",
        exit_state,
        side.get_stat(Stat::Time),
        side.get_stat(Stat::RobotBuild),
        side.get_stat(Stat::RobotKill),
        side.get_stat(Stat::TurretBuild),
        side.get_stat(Stat::TurretKill),
        side.get_stat(Stat::BuildingKill),
    );
    if let Err(e) = std::fs::write(&path, text) {
        log::error!("host result: failed to write {path}: {e}");
    }
}

/// Per-map Lang.dat RobotsMap briefing texts, paragraphs pre-joined.
struct MapTexts {
    begin: String,
    win: String,
    loose: String,
}

/// Slug of the loaded map's file stem — "MATRIX/MAP/ATOLL.CMAP" and
/// "assets/atoll.bundle?bv=11" both yield "atoll", "MANSION (E)" →
/// "mansion_e". Same rule as tools/lang_dat.py and pack_bundle.rs;
/// matches the `map\t<slug>` keys in maps.txt.
fn briefing_stem() -> String {
    let name = crate::matrix_game::map::map_name();
    let name = name.split('?').next().unwrap_or("");
    let name = name.rsplit(['/', '\\']).next().unwrap_or("");
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    let mut out = String::new();
    for c in stem.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

/// Persist the best completion time for the current map into
/// localStorage `matrixgame.records` (`slug=ms;…`) — the quick-launch
/// menu (index.html) shows it as the «Время:» column + medal.
#[cfg(target_arch = "wasm32")]
fn save_map_record(elapsed_ms: i64) {
    let Some(store) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) else {
        return;
    };
    let slug = briefing_stem();
    if slug.is_empty() {
        return;
    }
    let raw = store
        .get_item("matrixgame.records")
        .ok()
        .flatten()
        .unwrap_or_default();
    if let Some(s) = merge_record(&raw, &slug, elapsed_ms) {
        let _ = store.set_item("matrixgame.records", &s);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn save_map_record(_elapsed_ms: i64) {}

/// Fold a new completion time into the `slug=ms;…` records string;
/// None when the stored time is already at least as good.
#[cfg(any(target_arch = "wasm32", test))]
fn merge_record(raw: &str, slug: &str, elapsed_ms: i64) -> Option<String> {
    let mut entries: Vec<(String, i64)> = raw
        .split(';')
        .filter_map(|p| p.split_once('='))
        .filter_map(|(k, v)| v.parse().ok().map(|v| (k.to_string(), v)))
        .collect();
    match entries.iter_mut().find(|(k, _)| k == slug) {
        Some(e) if e.1 <= elapsed_ms => return None,
        Some(e) => e.1 = elapsed_ms,
        None => entries.push((slug.to_string(), elapsed_ms)),
    }
    Some(
        entries
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(";"),
    )
}

/// Parse the `tag\tvalue` lines of maps.txt for one map's entry.
fn parse_map_texts(raw: &str, stem: &str) -> Option<MapTexts> {
    let mut in_map = false;
    let (mut begin, mut win, mut loose) = (Vec::new(), Vec::new(), Vec::new());
    for line in raw.lines() {
        let Some((tag, val)) = line.split_once('\t') else {
            continue;
        };
        match tag {
            "map" => {
                if in_map {
                    break;
                }
                in_map = val == stem;
            }
            "begin" if in_map => begin.push(val),
            "win" if in_map => win.push(val),
            "loose" if in_map => loose.push(val),
            _ => {}
        }
    }
    (!begin.is_empty()).then(|| MapTexts {
        begin: begin.join("\n\n"),
        win: win.join("\n\n"),
        loose: loose.join("\n\n"),
    })
}

#[cfg(target_arch = "wasm32")]
async fn load_map_texts() -> Option<MapTexts> {
    let raw = crate::gfx::loader::load_bytes("assets/menu/maps.txt")
        .await
        .ok()?;
    let raw = String::from_utf8(raw).ok()?;
    let texts = parse_map_texts(&raw, &briefing_stem());
    if texts.is_none() {
        log::info!("briefing: no maps.txt entry for '{}'", briefing_stem());
    }
    texts
}

/// Start-of-game base selection + camera fallback — port of the
/// "Camera & Select" block of RS_CAMPOS (MatrixMapPrepare.cpp:951-985):
/// with `CamPos` present, select the player base standing near it;
/// without, select the first player base and aim the camera at the
/// point 100 units in front of it.
fn select_start_base(
    game: &mut crate::matrix_game::logic::MapLogic,
    camera: &mut crate::matrix_game::camera::Camera,
    map: &crate::matrix_game::map::GameMap,
) {
    use crate::matrix_game::map_static::MapStatic as _;
    use crate::matrix_game::object_building::BuildingType;
    use crate::matrix_game::side::CurrSel;
    let (si, co) = map.camera_angle.sin_cos();
    let player = game.player_side.id;
    let bases: Vec<(crate::matrix_game::map_static::ObjectId, glam::Vec2)> = game
        .objects
        .iter_units()
        .filter_map(|id| {
            let b = crate::matrix_game::logic::building_ref(&game.objects, id)?;
            (b.kind == BuildingType::Base && b.side == player && b.is_live())
                .then_some((id, glam::Vec2::new(b.pos.x, b.pos.y)))
        })
        .collect();
    for (bid, bpos) in bases {
        let bup = glam::Vec2::new(bpos.x - 100.0 * si, bpos.y + 100.0 * co);
        match map.camera_pos {
            Some(cp) => {
                if (bup - glam::Vec2::new(cp[0], cp[1])).length_squared() < 300.0 * 300.0 {
                    game.player_side.select_single(bid, CurrSel::BaseSelected);
                    break;
                }
            }
            None => {
                game.player_side.select_single(bid, CurrSel::BaseSelected);
                camera.set_xy_strategy([bup.x, bup.y]);
                break;
            }
        }
    }
}

/// Execute an armed pre-order at a minimap world position — the
/// `CalcMinimap2World` substitution at MatrixSide.cpp:672-680. Returns
/// false (order stays armed) for target-requiring orders the minimap
/// can't express (capture / repair), letting the click fall through.
fn execute_pre_order_world(state: &mut AppState, tgt: [f32; 2]) -> bool {
    use crate::matrix_game::interface::iface_list::PreOrder;

    let Some(po) = state.iface_list.pre_order else {
        return false;
    };
    let w = glam::Vec2::new(tgt[0], tgt[1]);
    match po {
        PreOrder::Move => state.game.order_move_to_world(&state.map, w),
        PreOrder::Fire => state.game.order_attack_world(&state.map, w),
        PreOrder::Patrol => state.game.order_patrol_world(&state.map, w),
        PreOrder::Bomb => state.game.order_bomb_world(&state.map, w),
        PreOrder::Capture | PreOrder::Repair => return false,
    }
    state.iface_list.pre_order = None;
    true
}

fn execute_pre_order(state: &mut AppState, cx: f32, cy: f32, w: f32, h: f32) {
    use crate::matrix_game::interface::iface_list::PreOrder;

    let Some(po) = state.iface_list.pre_order.take() else {
        return;
    };
    match po {
        PreOrder::Move => {
            state
                .game
                .order_move_to_at(&state.camera, cx, cy, w, h, &state.map);
        }
        PreOrder::Fire => {
            let hit = state
                .game
                .order_fire_at_screen(&state.camera, cx, cy, w, h, &state.map);
            log::debug!("order: attack dispatched (target: {:?})", hit);
        }
        PreOrder::Capture => {
            if !state
                .game
                .order_capture_at_screen(&state.camera, cx, cy, w, h, &state.map)
            {
                // Invalid target keeps the order armed (MatrixSide.cpp:715).
                state.iface_list.pre_order = Some(PreOrder::Capture);
            }
        }
        PreOrder::Patrol => {
            state
                .game
                .order_patrol_at_screen(&state.camera, cx, cy, w, h, &state.map);
        }
        PreOrder::Bomb => {
            state
                .game
                .order_bomb_at_screen(&state.camera, cx, cy, w, h, &state.map);
        }
        PreOrder::Repair => {
            if !state
                .game
                .order_repair_at_screen(&state.camera, cx, cy, w, h, &state.map)
            {
                state.iface_list.pre_order = Some(PreOrder::Repair);
            }
        }
    }
}

fn dispatch_ui_right_click(state: &mut AppState, click: &crate::matrix_game::interface::Click) {
    use crate::matrix_game::interface::iface_menu::popup_for_pylon;
    use crate::matrix_game::interface::Click;
    use crate::matrix_game::side::CurrSel;

    let name = match click {
        Click::RightButton(n) => n.as_str(),
        _ => return,
    };

    // The popup only opens while the constructor is active and the
    // selection is on a base. Mirrors the early-return guards at
    // CIFaceButton.cpp:188-189.
    let active = state
        .game
        .player_side
        .builder
        .as_ref()
        .map(|b| b.active)
        .unwrap_or(false);
    if !active || state.game.player_side.curr_sel != CurrSel::BaseSelected {
        return;
    }

    let strings = crate::matrix_game::config::global_strings();
    let base_panel = state.iface_list.panel("Base");
    if let Some(mut popup) = popup_for_pylon(name, base_panel, &strings.popup_none) {
        // CIFaceMenu::CreateMenu (CIFaceMenu.cpp:62-100) — record the
        // caller pylon, locate the cursik index for the equipped item.
        popup.set_caller(name);
        if let Some(b) = state.game.player_side.builder.as_ref() {
            popup.set_saved_config(*b.cfg());
            popup.refresh_current_pos(b);
            // CIFaceButton.cpp:190-310 — colour each row by affordability
            // before showing the menu.
            let mut bank = [0i32; 4];
            for r in crate::matrix_game::config::Resource::ALL {
                bank[r as usize] = state.game.player_side.get_resource_amount(r);
            }
            popup.refresh_affordability(b, &bank);
        }
        log::debug!("popup: opened for pylon {}", name);
        state.iface_list.popup_restore_pending = None;
        state.iface_list.popup = Some(popup);
    }
}

/// Returns `true` when the hover changed the live preview config so
/// the caller can reset the build counter (Djeans007 resets
/// `m_RCountControl` at CConstructor.cpp:578).
fn preview_popup_hover(
    builder: &mut crate::matrix_game::interface::constructor::RobotBuilder,
    popup: Option<&mut crate::matrix_game::interface::iface_menu::CIFaceMenu>,
) -> bool {
    let Some(popup) = popup else {
        return false;
    };
    if popup.previewed == popup.hovered {
        return false;
    }
    let Some(idx) = popup.hovered else {
        // Cursor left the popup rows — the C++ keeps showing the LAST
        // hovered component until commit/cancel (`Djeans007` only
        // fires on row hover, CConstructor.cpp:576-671; the restore
        // happens in ResetMenu on close). Leave the preview alone.
        return false;
    };
    popup.previewed = popup.hovered;
    let Some(item) = popup.items.get(idx).cloned() else {
        return false;
    };
    let ty = popup.parent.unit_type();
    builder.djeans007(ty, item.kind, popup.parent.pilon());
    // Port of the per-hover `RemoteFocusElement` fire in the C++
    // popup loop (CIFaceList::OnMouseMove + CConstructor.cpp:912-958).
    // Updates the focused label/description/price card on the Base
    // panel so hovering a popup row gives the same readouts as
    // hovering the equivalent template button would.
    builder.set_labels_and_price(ty, item.kind);
    true
}

/// Seed dynamic `[key]` replacement values on `IFaceList::hint_replacer`
/// based on the hovered element name. Port of
/// `CIFaceList::AddHintReplacements` (CInterface.cpp:4439-4540) — we
/// only populate values for the element the pointer currently owns so
/// the map stays lean and the per-frame cost is O(1).
///
/// The original mutates the global `PAR_REPLACE` block in place;
/// `HintReplacer` is the Rust equivalent. Values that depend on live
/// state (resource income, robot counts) are refreshed every frame so
/// a long-held hover shows up-to-date numbers.
fn refresh_hint_replacements(state: &mut AppState) {
    use crate::matrix_game::config::Resource;

    // `HintSystem::update` early-returns when nothing is hovered, so
    // we can skip the full refresh when the focused element has no
    // hint. Cheaper than rebuilding the income query every frame.
    let Some((pi, ei)) = state.iface_list.focused else {
        return;
    };
    let Some(panel) = state.iface_list.panels.get(pi) else {
        return;
    };
    let Some(elem) = panel.elements.get(ei) else {
        return;
    };
    if elem.hint_template.is_empty() {
        return;
    }
    let elem_name = elem.name.clone();
    let side_id = state.game.player_side.id;
    let repl = &mut state.iface_list.hint_replacer;
    match elem_name.as_str() {
        "thz" => {
            let (base_i, fa_i) = state.game.compute_resource_income(side_id, Resource::Titan);
            repl.set("_titan_income", (base_i + fa_i).to_string());
        }
        "enhz1" | "enhz2" => {
            let (base_i, fa_i) = state
                .game
                .compute_resource_income(side_id, Resource::Energy);
            repl.set("_energy_income", (base_i + fa_i).to_string());
        }
        "elhz" => {
            let (base_i, fa_i) = state
                .game
                .compute_resource_income(side_id, Resource::Electronics);
            repl.set("_electronics_income", (base_i + fa_i).to_string());
        }
        "phz" => {
            let (base_i, fa_i) = state
                .game
                .compute_resource_income(side_id, Resource::Plasma);
            repl.set("_plasma_income", (base_i + fa_i).to_string());
        }
        "rvhz" => {
            let total = state.game.player_side.robots_cnt;
            let max = state.game.compute_max_side_robots(side_id);
            repl.set("_total_robots", total.to_string());
            repl.set("_max_robots", max.to_string());
        }
        "tur1" | "tur2" | "tur3" | "tur4" => {
            // Port of CInterface.cpp:4463-4519 turret hint replacements.
            // One call per turret slot; the template `BuildTurret`
            // references `_turret_name`, `_turret_range`,
            // `_turret_structure`, `_turret_damage`, `_turret_res1..4`.
            let idx = match elem_name.as_str() {
                "tur1" => 0,
                "tur2" => 1,
                "tur3" => 2,
                "tur4" => 3,
                _ => unreachable!(),
            };
            let (name_label, range_label) = state.iface_list.hint_replacer.turret_label(idx);
            let name_label = name_label.to_string();
            let range_label = range_label.to_string();
            let cfg = crate::matrix_game::config::global();
            let cannon = cfg.turrets.cannons[idx];
            // Structure: hitpoint / 10 (matches CInterface.cpp:4467).
            let structure = (cannon.hitpoint / 10.0) as i32;
            // Damage per second — shots/sec × per-shot damage, then
            // /10 to match the UI's display scale. Mirrors
            // CInterface.cpp:4464 `damage = (1 / (cooldown/1000)) *
            // m_RobotDamages[…].damage`.
            use crate::matrix_game::effects::weapon::{
                weap_to_index, WEAPON_CANNON0, WEAPON_CANNON1, WEAPON_CANNON2, WEAPON_CANNON3,
            };
            let cannon_weap = [
                WEAPON_CANNON0,
                WEAPON_CANNON1,
                WEAPON_CANNON2,
                WEAPON_CANNON3,
            ][idx];
            let cannon_idx = weap_to_index(cannon_weap).unwrap_or(0);
            let cooldown_ms = cfg.weapon_cooldown.table[cannon_idx];
            let per_shot_damage = cfg.robot_damages.table[cannon_idx].damage;
            // `Float2Int(1/(cooldown/1000))` FIRST, then × damage
            // (CInterface.cpp:4464) — rounding order changes the
            // displayed numbers for turrets 1 and 4.
            let dps = if cooldown_ms > 0 {
                crate::matrix_game::common::float2int(1000.0 / cooldown_ms as f32)
                    * per_shot_damage
            } else {
                0
            };
            let dmg10 = dps / 10;
            // Turret 1 + 4 render their damage as "X+X" in the hint
            // (CInterface.cpp:4468 / :4510) — both burst-weapons in
            // the shipped game; the other two use the plain value.
            let damage_str = if matches!(idx, 0 | 3) {
                format!("{dmg10}+{dmg10}")
            } else {
                dmg10.to_string()
            };
            let repl = &mut state.iface_list.hint_replacer;
            repl.set("_turret_name", name_label);
            repl.set("_turret_range", range_label);
            repl.set("_turret_structure", structure.to_string());
            repl.set("_turret_damage", damage_str);
            for i in 0..Resource::ALL.len() {
                let v = cannon.resources[i];
                let key = format!("_turret_res{}", i + 1);
                if v > 0 {
                    repl.set(key, v.to_string());
                } else {
                    repl.set(key, String::new());
                }
            }
        }
        // IF_CALL_FROM_HELL — maintenance availability + countdown
        // (CInterface.cpp:4520-4539).
        "callhell" | "bure" => {
            let repl = &mut state.iface_list.hint_replacer;
            repl.set("_ch_cant", String::new());
            repl.set("_ch_can", String::new());
            repl.set("_ch_time_min", String::new());
            repl.set("_ch_time_sec", String::new());
            if state.game.maintenance_disabled() {
                state
                    .iface_list
                    .hint_replacer
                    .set("_ch_cant", "1".to_string());
            } else if state.game.maintenance_time > 0 {
                let ms = state.game.maintenance_time;
                let minutes = ms / 60000;
                let seconds = ms / 1000 - minutes * 60;
                let repl = &mut state.iface_list.hint_replacer;
                repl.set("_ch_time_min", minutes.max(0).to_string());
                repl.set("_ch_time_sec", seconds.max(0).to_string());
            } else {
                state
                    .iface_list
                    .hint_replacer
                    .set("_ch_can", "1".to_string());
            }
        }
        _ => {}
    }
}

/// Advance the constructor preview turntable while the panel is open.
fn tick_builder_preview(state: &mut AppState, step_ms: f32) {
    let active = state
        .game
        .player_side
        .builder
        .as_ref()
        .map(|b| b.active)
        .unwrap_or(false);
    if !active {
        return;
    }
    state.builder_preview.tick(step_ms);
}

/// Per-frame snapshot of what the constructor preview wants drawn.
/// Mirrors the multi-unit `m_Robot->Draw()` walk at
/// MatrixObjectRobot.cpp:319-356 — chassis is required, armor / head /
/// weapons stack above when populated.
pub struct BuilderPreviewQuery {
    pub chassis: crate::matrix_game::robot::ChassisKind,
    pub armor_kind: Option<i32>,
    pub head_kind: Option<i32>,
    pub weapon_kinds: [Option<i32>; 5],
    pub angle_rad: f32,
    pub design_rect: [f32; 4],
}

/// Query helper for the render pass — returns the preview chassis +
/// armor/head/weapon kinds + turntable angle + design-space viewport
/// rect when the panel is open and the live config has a chassis
/// selected. Returns `None` when nothing should be drawn (panel
/// closed or chassis empty).
fn builder_preview_query(state: &mut AppState) -> Option<BuilderPreviewQuery> {
    let active = state
        .game
        .player_side
        .builder
        .as_ref()
        .map(|b| b.active)
        .unwrap_or(false);
    if !active {
        return None;
    }
    // Preview viewport rect — port of `SetRenderProps` reading the
    // `Const{X,Y,Width,Height}` panel-level params at
    // CInterface.cpp:208-213. The C++ adds these to the panel's
    // resolved screen origin to get the absolute viewport rect; we
    // do the equivalent in `render_preview_full` (panel.resolved_pos
    // + design_rect * scale).
    let design_rect = state
        .iface_list
        .panel("Base")
        .and_then(|p| p.const_rect)
        .unwrap_or([426.0, 56.0, 221.0, 314.0]);
    let cfg = *state.game.player_side.builder.as_ref()?.cfg();
    let ticket = state.builder_preview.ticket(&cfg, design_rect)?;
    let kind_or_none = |k: i32| if k >= 1 { Some(k) } else { None };
    Some(BuilderPreviewQuery {
        chassis: ticket.chassis,
        armor_kind: kind_or_none(cfg.hull.unit.kind.0),
        head_kind: kind_or_none(cfg.head.kind.0),
        weapon_kinds: [
            kind_or_none(cfg.weapon[0].kind.0),
            kind_or_none(cfg.weapon[1].kind.0),
            kind_or_none(cfg.weapon[2].kind.0),
            kind_or_none(cfg.weapon[3].kind.0),
            kind_or_none(cfg.weapon[4].kind.0),
        ],
        angle_rad: ticket.rotation_rad,
        design_rect: ticket.design_rect,
    })
}

/// Per-frame placement-preview tick — port of
/// `CMatrixSideUnit::LogicTakt`'s BUILDING_TURRET branch
/// (MatrixSide.cpp:528-585). Returns a `GhostCannon` to render when a
/// turret-build session is active; `None` otherwise.
///
/// Steps (all mirrored from the C++):
///   1. Ray-cast the cursor onto the terrain plane at the parent base's
///      `build_z`. (The C++ traces against landscape+water; intersecting
///      a flat plane is good enough since slots are at base elevation.)
///   2. Walk the parent's `turret_places[]` and find the nearest free
///      slot inside `SNAP_DIST` (4 move-cells in C++ → 40 world units).
///   3. Set `can_build` = `turrets_have < turrets_max` AND a slot was
///      hovered AND the player has the required resources.
///   4. Snap ghost pos to the slot if hovered, otherwise to the raw
///      cursor; smooth-rotate ghost angle toward the slot's rest angle
///      using the C++ damping `dang * (1 - 0.99^ms)`.
fn update_turret_build(
    state: &mut AppState,
    step_ms: i32,
) -> (
    Option<crate::matrix_game::object_cannon::GhostCannon>,
    Vec<crate::matrix_game::slot_marker::SlotMarker>,
) {
    use crate::matrix_game::config::Resource;
    use crate::matrix_game::map_static::{MapStatic, ObjectType};
    use crate::matrix_game::object_building::Building;
    use crate::matrix_game::object_cannon::GhostCannon;
    use crate::matrix_game::slot_marker::SlotMarker;

    if !state.iface_list.turret_build.is_active() {
        return (None, Vec::new());
    }
    let Some(parent_id) = state.iface_list.turret_build.parent else {
        return (None, Vec::new());
    };

    // Snapshot the parent's slot table so the cursor scan doesn't fight
    // the borrow on `state.iface_list.turret_build`.
    let (parent_pos, parent_z, slots, turrets_have, turrets_max, parent_side) = {
        let Some(obj) = state.game.objects.get(parent_id) else {
            return (None, Vec::new());
        };
        if !matches!(obj.core().obj_type, ObjectType::Building) {
            return (None, Vec::new());
        }
        let b: &Building = unsafe { &*(obj as *const dyn MapStatic as *const Building) };
        (
            b.pos,
            b.build_z,
            b.turret_places.clone(),
            b.turrets_have,
            b.turrets_max,
            b.side,
        )
    };

    // Refuse builds on enemy bases up front — same gate as the click
    // path (CConstructor.cpp:225-227).
    if parent_side != state.game.player_side.id {
        return (None, Vec::new());
    }

    // Slot markers — port of `CreatePlacesShow`
    // (MatrixObjectBuilding.cpp:1617). Visible whenever the picker
    // is open (regardless of whether a kind has been committed) so
    // the player can see where they may build before clicking.
    // Spot radius — port of MatrixEffectLandscapeSpot.cpp:233-236:
    //   xc = SPOT_SIZE * scalex * cos(angle)
    // SPOT_SIZE = 5.0 (MatrixEffectLandscapeSpot.hpp:22), and the
    // scale passed at MatrixObjectBuilding.cpp:1640 is 6.0. So the
    // quad's half-extent is 30 world units.
    const SPOT_SIZE: f32 = 5.0;
    const SPOT_TURRET_SCALE: f32 = 6.0;
    let radius = SPOT_SIZE * SPOT_TURRET_SCALE;
    let markers: Vec<SlotMarker> = slots
        .iter()
        .filter(|p| p.cannon_type < 0)
        .map(|p| {
            // Sample terrain at the four corners + center; lift the
            // marker above the highest of them so corners don't sink
            // below uneven terrain (which clips the decal because it's
            // a single flat quad rather than a true terrain-conforming
            // decal).
            let cx = p.world.x;
            let cy = p.world.y;
            let r = radius;
            let z_max = state
                .map
                .get_z(cx, cy)
                .max(state.map.get_z(cx + r, cy + r))
                .max(state.map.get_z(cx - r, cy + r))
                .max(state.map.get_z(cx + r, cy - r))
                .max(state.map.get_z(cx - r, cy - r))
                .max(parent_z);
            SlotMarker {
                pos: p.world,
                pos_z: z_max + 1.0,
                radius,
            }
        })
        .collect();

    // No kind chosen yet → markers only, no ghost.
    let Some(kind) = state.iface_list.turret_build.kind.map(|k| k as i32) else {
        return (None, markers);
    };

    // Ray-cast cursor onto z=parent_z plane.
    let [cx, cy] = state.cursor;
    let w = state.gfx.config.width as f32;
    let h = state.gfx.config.height as f32;
    let (origin, dir) = state.camera.screen_to_world_ray(cx, cy, w, h);
    let cursor_world = if dir.z.abs() > 1e-6 {
        let t = (parent_z - origin.z) / dir.z;
        if t > 0.0 {
            glam::Vec2::new(origin.x + dir.x * t, origin.y + dir.y * t)
        } else {
            parent_pos
        }
    } else {
        parent_pos
    };

    // Snap to nearest free slot — C++ uses `rr < 4` in move-cell space
    // (MatrixSide.cpp:1645): dist² < 4 cells² → dist < 2 cells = 20
    // world units.
    const SNAP_DIST: f32 = 2.0 * crate::matrix_game::map::GameMap::GLOBAL_SCALE_MOVE;
    const SNAP_DIST_SQ: f32 = SNAP_DIST * SNAP_DIST;
    let mut hovered: Option<(usize, f32)> = None;
    for (i, p) in slots.iter().enumerate() {
        if p.cannon_type > 0 {
            continue; // occupied
        }
        let d2 = (p.world - cursor_world).length_squared();
        if d2 < SNAP_DIST_SQ && hovered.map_or(true, |(_, best)| d2 < best) {
            hovered = Some((i, d2));
        }
    }

    // Resource check — port of `IsEnoughResources` (CSide.cpp:?).
    let cost = crate::matrix_game::config::global().turrets.cost_of(kind);
    let resources_ok = Resource::ALL
        .iter()
        .all(|r| state.game.player_side.get_resource_amount(*r) >= cost.resources[*r as usize]);

    let can_build = hovered.is_some() && turrets_have < turrets_max && resources_ok;

    let (target_pos, target_angle) = if let Some((i, _)) = hovered {
        let p = &slots[i];
        (p.world, p.angle)
    } else {
        (cursor_world, state.iface_list.turret_build.ghost_angle)
    };
    // Ghost Z: the terrain under the slot, like the C++ cannon RNeed
    // (4-point average, m_AddH=0 for slot turrets — MatrixSide.cpp:6009).
    let ghost_z = state.map.point_avg_z(target_pos.x, target_pos.y);

    let tb = &mut state.iface_list.turret_build;
    tb.cursor_world = (cursor_world.x, cursor_world.y);
    tb.hovered_slot = hovered.map(|(i, _)| i as i32);
    tb.can_build = can_build;
    tb.ghost_pos = target_pos;
    tb.ghost_z = ghost_z;

    // Smooth-rotate to the slot's rest angle. Port of MatrixSide.cpp:578:
    //   m_Angle += dang * (1 - 0.99^ms)
    // where `dang` is the shortest-arc angle from current to target.
    let dang = shortest_arc(tb.ghost_angle, target_angle);
    let damping = 1.0 - (0.99_f64).powf(step_ms.max(0) as f64) as f32;
    tb.ghost_angle += dang * damping;

    let ghost = GhostCannon {
        kind,
        pos: tb.ghost_pos,
        pos_z: tb.ghost_z,
        angle: tb.ghost_angle,
        can_build: tb.can_build,
        side: state.game.player_side.id,
    };
    (Some(ghost), markers)
}

/// Shortest signed angle from `a` to `b` in radians, in the range
/// `(-π, π]`. Port of the C++ `AngleDist` helper used at
/// MatrixSide.cpp:576.
fn shortest_arc(a: f32, b: f32) -> f32 {
    let two_pi = std::f32::consts::TAU;
    let mut d = (b - a) % two_pi;
    if d > std::f32::consts::PI {
        d -= two_pi;
    } else if d < -std::f32::consts::PI {
        d += two_pi;
    }
    d
}

/// Port of the "click during PREORDER_BUILD_TURRET" path
/// (MatrixFormGame.cpp:1498-1512). If the click lands on the parent
/// base's ring, queue the turret + deduct cost; otherwise cancel.
fn try_place_turret(state: &mut AppState, _cx: f32, _cy: f32, _w: f32, _h: f32) {
    use crate::matrix_game::config::Resource;
    use crate::matrix_game::map_static::{MapStatic, ObjectType};
    use crate::matrix_game::object_building::Building;

    let Some(parent_id) = state.iface_list.turret_build.parent else {
        state.iface_list.turret_build.cancel();
        return;
    };
    let Some(kind) = state.iface_list.turret_build.kind else {
        state.iface_list.turret_build.cancel();
        return;
    };

    // The per-frame `update_turret_build` already snapped the cursor to
    // a slot (or determined no slot is hovered). Honour its decision —
    // the C++ click path also reads `m_CannonForBuild.m_CanBuildFlag`
    // set by the LogicTakt branch (MatrixSide.cpp:626). A click off any
    // slot keeps the placement mode armed (MatrixSide.cpp:665-667 only
    // places when the flag is up; RMB / UI buttons cancel).
    if !state.iface_list.turret_build.can_build {
        log::debug!("turret: click without valid slot — placement stays armed");
        return;
    }
    let Some(slot_idx) = state.iface_list.turret_build.hovered_slot else {
        return;
    };
    let ghost_pos = state.iface_list.turret_build.ghost_pos;
    let ghost_z = state.iface_list.turret_build.ghost_z;
    let ghost_angle = state.iface_list.turret_build.ghost_angle;

    let turret_cost = crate::matrix_game::config::global()
        .turrets
        .cost_of(kind as i32);

    let placement_ok = {
        let Some(obj) = state.game.objects.get_mut(parent_id) else {
            state.iface_list.turret_build.cancel();
            return;
        };
        if !matches!(obj.core().obj_type, ObjectType::Building) {
            state.iface_list.turret_build.cancel();
            return;
        }
        let b: &mut Building = unsafe { &mut *(obj as *mut dyn MapStatic as *mut Building) };
        if b.side != state.game.player_side.id {
            state.iface_list.turret_build.cancel();
            return;
        }
        // Reserve the slot — flips m_CannonType to the kind so subsequent
        // hovers in the same session won't snap to it.
        b.queue_turret_slot(slot_idx, kind as i32)
    };
    if !placement_ok {
        log::debug!("turret: slot {} no longer free — cancelling", slot_idx);
        state.iface_list.turret_build.cancel();
        return;
    }
    // S_TURRET_BUILD_START (MatrixSide.cpp:659).
    state.game.objects.queue_snd("t_build_s");

    // Spawn the Cannon object immediately — the build-stack timer
    // still runs for the cost/progress UI, but the C++ mounts the
    // cannon on the building as soon as BeginBuildTurret commits.
    // HP starts at 0 + state UNDER_CONSTRUCTION + invulnerable; the
    // build-stack tick ramps HP up over `turret_build_time_ms`.
    let mut cannon = crate::matrix_game::object_cannon::Cannon::new(
        ghost_pos,
        ghost_z,
        ghost_angle,
        state.game.player_side.id,
        kind as i32,
        Some(parent_id),
        slot_idx,
    );
    cannon.begin_construction();
    // `m_Place = m_RN.FindInPL(...)` at placement (MatrixSide.cpp:563).
    cannon.place =
        crate::matrix_game::logic::cannon_rn_place(&state.map, ghost_pos.x, ghost_pos.y);
    let id = state.game.objects.spawn(Box::new(cannon));
    if let Some(obj) = state.game.objects.get_mut(id) {
        let c: &mut crate::matrix_game::object_cannon::Cannon = unsafe {
            &mut *(obj as *mut dyn crate::matrix_game::map_static::MapStatic
                as *mut crate::matrix_game::object_cannon::Cannon)
        };
        c.self_id = Some(id);
    }
    state.game.objects.add_lt(id);

    for r in Resource::ALL {
        state
            .game
            .player_side
            .add_resource_amount(r, -turret_cost.resources[r as usize]);
    }
    log::debug!(
        "turret: placed {:?} on slot {} as object {:?}",
        kind,
        slot_idx,
        id,
    );
    state.iface_list.turret_build.cancel();
}

/// Build the `CheckUpCtx` (CCounter.cpp:66-99 inputs) for the
/// build-multiplier counter. Reads the player side's per-resource pool
/// and the live preview's per-unit cost. The remaining fields are
/// computed by scanning the live object list so the counter and build
/// button honor robot-cap and base-stack limits like the C++.
fn build_counter_ctx(state: &AppState) -> crate::matrix_game::interface::counter::CheckUpCtx {
    use crate::matrix_game::config::Resource;
    use crate::matrix_game::interface::constructor::UnitPrice;
    use crate::matrix_game::interface::counter::CheckUpCtx;
    use crate::matrix_game::map_static::{MapStatic, ObjectType};
    use crate::matrix_game::object_building::{Building, BuildingType};
    use crate::matrix_game::robot::Robot;
    let per_unit_price = state
        .game
        .player_side
        .builder
        .as_ref()
        .map(|b| b.construction_price())
        .unwrap_or_else(UnitPrice::zero);
    let mut side_resources = [0i32; 4];
    for r in Resource::ALL {
        side_resources[r as usize] = state.game.player_side.get_resource_amount(r);
    }
    let mut side_robots = 0;
    let mut robots_in_stack = 0;
    let mut bases = 0;
    let mut factories = 0;
    state
        .game
        .objects
        .for_each_live(|_, obj| match obj.core().obj_type {
            ObjectType::RobotAi => {
                let r: &Robot = unsafe { &*(obj as *const dyn MapStatic as *const Robot) };
                // DIP (dying) robots don't count toward the cap
                // (`m_CurrState != ROBOT_DIP`, MatrixSide.cpp:591).
                if r.side == state.game.player_side.id && r.is_live() {
                    side_robots += 1;
                }
            }
            ObjectType::Building => {
                let b: &Building = unsafe { &*(obj as *const dyn MapStatic as *const Building) };
                if b.side == state.game.player_side.id {
                    if b.kind == BuildingType::Base {
                        bases += 1;
                        // Queued turrets aren't robots
                        // (CBuildStack::GetRobotsCnt filters
                        // OBJECT_TYPE_ROBOTAI, MatrixObjectBuilding.cpp:1938-1950).
                        robots_in_stack += b.build_stack.robots_cnt() as i32;
                    } else {
                        factories += 1;
                    }
                }
            }
            _ => {}
        });
    let active_base_stack_items = state
        .game
        .active_object()
        .and_then(|id| state.game.objects.get(id))
        .filter(|o| matches!(o.core().obj_type, ObjectType::Building))
        .and_then(|obj| {
            let b: &Building = unsafe { &*(obj as *const dyn MapStatic as *const Building) };
            (b.kind == BuildingType::Base).then_some(b.build_stack.items() as i32)
        });
    CheckUpCtx {
        per_unit_price,
        side_resources,
        side_robots,
        robots_in_stack,
        max_side_robots: bases * 3 + if bases == 0 { 0 } else { 4 } + factories,
        active_base_stack_items,
    }
}

/// Port of the `m_RCountControl->Reset()` at the top of every
/// `SuperDjeans` / `Djeans007` (CConstructor.cpp:443,578) plus the
/// trailing `m_RCountControl->CheckUp()` — every component change
/// resets the build multiplier to 1 and revalidates it against the
/// side's resources/caps. Called on each dispatch path that mutates
/// the constructor's configuration.
fn reset_build_counter(state: &mut AppState) {
    state.iface_list.r_count_control.reset();
    let ctx = build_counter_ctx(state);
    state.iface_list.r_count_control.check_up(ctx);
}

/// Port of `CConstructor::RemoteBuild` (CConstructor.cpp:223-250).
fn commit_and_queue_robot(state: &mut AppState) {
    use crate::matrix_game::config::Resource;
    use crate::matrix_game::map_static::{MapStatic, ObjectType};
    use crate::matrix_game::object_building::{Building, BuildingType};
    use crate::matrix_game::side::CurrSel;

    if state.game.player_side.curr_sel != CurrSel::BaseSelected {
        log::debug!("cobuild: no base selected");
        return;
    }
    let Some(id) = state.game.active_object() else {
        return;
    };

    // CConstructor.cpp:225-227 — player-side guard.
    // No upfront affordability check: the C++ relies on
    // `CIFaceCounter::CheckUp` to cap the counter to what the player
    // can afford / fit in the base stack. The cobuild button is also
    // disabled when `build_enabled` is false (see refresh visibility).
    let (cfg, price, count) = {
        let Some(b) = state.game.player_side.builder.as_ref() else {
            return;
        };
        (
            *b.cfg(),
            b.construction_price(),
            state.iface_list.r_count_control.counter().max(1),
        )
    };

    let Some(obj) = state.game.objects.get_mut(id) else {
        return;
    };
    if !matches!(obj.core().obj_type, ObjectType::Building) {
        return;
    }
    let b: &mut Building = unsafe { &mut *(obj as *mut dyn MapStatic as *mut Building) };
    if b.kind != BuildingType::Base {
        return;
    }
    // Port of `CConstructor::RemoteBuild`'s guard at CConstructor.cpp:
    // 225-227 — `m_Base->m_Side != PLAYER_SIDE` returns early, so you
    // can't queue robots on enemy / neutral bases even if the panel is
    // somehow open.
    if b.side != state.game.player_side.id {
        log::debug!(
            "cobuild: refused — base side {} != player side {}",
            b.side,
            state.game.player_side.id
        );
        return;
    }
    // CConstructor.cpp:233-235 — push `counter` robots onto the base
    // stack (StackRobot drops the request if the stack is full).
    let mut queued = 0;
    for _ in 0..count {
        if !b.queue_robot(cfg) {
            break;
        }
        queued += 1;
    }
    // CConstructor.cpp:237-242 — deduct price × counter. The C++
    // deducts blindly because the stack-full case is unreachable there
    // (the build button disables at MAX_STACK_UNITS); charge only what
    // was actually queued so a dropped item can't burn resources.
    for r in Resource::ALL {
        let spent = price.resources[r as usize] * queued;
        state.game.player_side.add_resource_amount(r, -spent);
    }
    // CConstructor.cpp:231 — push to global config history.
    state.iface_list.history.add(cfg);
    // CConstructor.cpp:244-246 — close the panel via ResetGroupNClose.
    // That call only clears `m_Active`; it does NOT reset the live
    // construction state. `RemoteBuild` itself stacks robots with
    // `StackRobot` (CConstructor.cpp:233-235) which never resets either,
    // so the user's design persists across builds.
    if let Some(b) = state.game.player_side.builder.as_mut() {
        b.deactivate();
    }
    state.is_paused = false;
    // CConstructor.cpp:247-248 — reset + revalidate the counter.
    state.iface_list.r_count_control.reset();
    let ctx = build_counter_ctx(state);
    state.iface_list.r_count_control.check_up(ctx);
    log::debug!(
        "cobuild: queued {} (chas={},hull={},head={}); left T/E/En/P={}/{}/{}/{}",
        queued,
        cfg.chassis.kind.0,
        cfg.hull.unit.kind.0,
        cfg.head.kind.0,
        state.game.player_side.get_resource_amount(Resource::Titan),
        state
            .game
            .player_side
            .get_resource_amount(Resource::Electronics),
        state.game.player_side.get_resource_amount(Resource::Energy),
        state.game.player_side.get_resource_amount(Resource::Plasma),
    );
}

/// Queue progress bars for the active building's build stack. Ports
/// the `m_BS.m_PB.Modify(...) + CreateClone(PBC_CLONE1, x, y, 87)`
/// dispatch at MatrixObjectBuilding.cpp:1681-1689 — a 87-pixel-wide
/// bar drawn over the `if/Main` panel coords that CInterface resolves
/// at `(m_xPos+283, m_yPos+71)`.  We route through
/// `CInterface::element_rect("prog")` so the bar lands exactly where
/// the Static the C++ draws underneath it lives.
fn refresh_progress_bars(state: &mut AppState) {
    use crate::matrix_game::map_static::{MapStatic, ObjectType};
    use crate::matrix_game::object_building::Building;
    use crate::matrix_game::progress_bar::ProgressBar;
    use crate::matrix_game::side::CurrSel;

    state.progress_bars.clear();

    // ── Floating HP bars over robots / turrets / buildings ──────────
    // Skipped while paused: the C++ hover trace lives in CMatrixMap::
    // Takt (MatrixMap.cpp:1150-1178), which the pause early-return
    // skips — so with the constructor (Pause(true)) or a dialog open,
    // world HP bars neither arm on hover nor draw over the window.
    if !state.is_paused {
        use crate::matrix_game::common::TRACE_LANDSCAPE;
        use crate::matrix_game::interface::interface::DESIGN_H;
        use crate::matrix_game::map_trace::{trace, TraceStop};

        let h = state.gfx.config.height as f32;
        let scale = (h / DESIGN_H).max(0.1);
        let bar_h = {
            let d = state.progress_bars.bar_height_design();
            if d > 0.0 {
                d
            } else {
                8.0
            }
        };

        // Arm the bar of the object under the cursor for 1s — reads
        // the shared per-frame mouse trace (MatrixMap.cpp:1150-1178,
        // maintained by `update_mouse_trace`).
        if let TraceStop::Object(id) = state.mouse_trace {
            if let Some(obj) = state.game.objects.get_mut(id) {
                obj.show_hitpoint();
            }
        }

        // Collect every armed bar (`BeforeDraw` PB blocks), gated by a
        // landscape LOS trace from the camera like the originals.
        let eye = state.camera.eye_pos_world();
        // Map objects included: terron / special break props carry
        // persistent damage bars (MatrixObject.cpp:810-877).
        let ids: Vec<_> = state
            .game
            .objects
            .iter_units()
            .chain(state.game.objects.iter_mapobjects())
            .collect();
        for id in ids {
            let Some(obj) = state.game.objects.get(id) else {
                continue;
            };
            let Some(bar) = obj.hitpoint_bar(&state.map) else {
                continue;
            };
            let (stop, _) = trace(
                &state.map,
                &state.game.objects,
                eye,
                bar.anchor,
                TRACE_LANDSCAPE,
                None,
            );
            if stop != TraceStop::None {
                continue;
            }
            // Projection happens at upload time with the render
            // camera; only the screen-pixel offsets are fixed here.
            state
                .progress_bars
                .push_world(crate::matrix_game::progress_bar::WorldBar {
                    anchor: bar.anchor,
                    x_off: bar.x_off * scale,
                    y_off: bar.y_off * scale,
                    width: bar.width * scale,
                    height: bar_h * scale,
                    fill: bar.fill,
                });
        }
    }

    // Robot-selection panel clones (CInterface.cpp:1556-1565): the
    // active robot's HP bar at (+68, +179) width 68 — same slot the
    // building HP clone uses — plus, with more than one robot
    // selected, a small 46px bar under each group icon
    // (`CreateProgressBarClone(icon_x, icon_y+36, 46, PBC_CLONE1)`).
    if matches!(state.game.player_side.curr_sel, CurrSel::RobotsSelected) {
        use crate::matrix_game::interface::interface::DESIGN_H;
        use crate::matrix_game::robot::Robot;

        let w = state.gfx.config.width as f32;
        let h = state.gfx.config.height as f32;
        let scale = (h / DESIGN_H).max(0.1);
        let bar_h = {
            let d = state.progress_bars.bar_height_design();
            if d > 0.0 {
                d
            } else {
                8.0
            }
        };
        if let Some(main) = state.iface_list.panel("Main") {
            let [panel_x, panel_y] = main.resolved_pos(w, h, scale);
            let ids = state.game.player_side.selected.clone();
            let mut fills: Vec<f32> = Vec::with_capacity(ids.len());
            let mut sel_idx = 0usize;
            for id in &ids {
                let Some(obj) = state.game.objects.get(*id) else {
                    continue;
                };
                if !matches!(obj.core().obj_type, ObjectType::RobotAi) {
                    continue;
                }
                let r: &Robot = unsafe { &*(obj as *const dyn MapStatic as *const Robot) };
                if state.game.player_side.active_object == Some(*id) {
                    sel_idx = fills.len();
                }
                fills.push((r.hit_point / r.hit_point_max.max(1.0)).clamp(0.0, 1.0));
            }
            // Group-icon grid constants — must match the `_dyngroup_`
            // layout in interface.rs (CInterface.cpp:3678-3697).
            const GROUP_X0: f32 = 225.0;
            const GROUP_Y0: f32 = 49.0;
            const GROUP_DX: f32 = 48.0;
            const GROUP_DY: f32 = 49.0;
            for (i, fill) in fills.iter().enumerate().take(9) {
                if i == sel_idx {
                    state.progress_bars.push(ProgressBar {
                        rect: [
                            panel_x + 68.0 * scale,
                            panel_y + 179.0 * scale,
                            68.0 * scale,
                            bar_h * scale,
                        ],
                        fill: *fill,
                    });
                }
                if fills.len() > 1 {
                    let col = (i % 3) as f32;
                    let row = (i / 3) as f32;
                    state.progress_bars.push(ProgressBar {
                        rect: [
                            panel_x + (GROUP_X0 + col * GROUP_DX) * scale,
                            panel_y + (GROUP_Y0 + row * GROUP_DY + 36.0) * scale,
                            46.0 * scale,
                            bar_h * scale,
                        ],
                        fill: *fill,
                    });
                }
            }
        }
    }

    // Only fires for a selected building — matches the C++ dispatch
    // at CInterface.cpp:1576-1578 (HP-bar clone) and
    // MatrixObjectBuilding.cpp:1685-1689 (build-progress clone).
    if !matches!(
        state.game.player_side.curr_sel,
        CurrSel::BaseSelected | CurrSel::BuildingSelected
    ) {
        return;
    }
    let Some(id) = state.game.active_object() else {
        return;
    };
    let Some(obj) = state.game.objects.get(id) else {
        return;
    };
    if !matches!(obj.core().obj_type, ObjectType::Building) {
        return;
    }
    let b: &Building = unsafe { &*(obj as *const dyn MapStatic as *const Building) };

    let w = state.gfx.config.width as f32;
    let h = state.gfx.config.height as f32;
    let Some(main) = state.iface_list.panel("Main") else {
        log::warn!("progress: no Main panel");
        return;
    };

    use crate::matrix_game::interface::interface::DESIGN_H;
    let scale = (h / DESIGN_H).max(0.1);
    let [panel_x, panel_y] = main.resolved_pos(w, h, scale);
    let bar_h_design = {
        let d = state.progress_bars.bar_height_design();
        if d > 0.0 {
            d
        } else {
            16.0
        }
    };

    // Port of CInterface.cpp:1578 — `bld->CreateProgressBarClone(
    // m_xPos+68, m_yPos+179, 68, PBC_CLONE2)`. HP bar, always visible
    // when a building is selected, width 68 at (+68, +179).
    const HP_OFFSET_X: f32 = 68.0;
    const HP_OFFSET_Y: f32 = 179.0;
    const HP_WIDTH: f32 = 68.0;
    let hp_fill = if b.hit_point_max > 0.0 {
        (b.hit_point / b.hit_point_max).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let hp_rect = [
        panel_x + HP_OFFSET_X * scale,
        panel_y + HP_OFFSET_Y * scale,
        HP_WIDTH * scale,
        bar_h_design * scale,
    ];
    state.progress_bars.push(ProgressBar {
        rect: hp_rect,
        fill: hp_fill,
    });

    // Port of MatrixObjectBuilding.cpp:1676-1689 — build-queue
    // progress bar at (+283, +71), width 87, PBC_CLONE1. Only when an
    // item is queued for construction.
    if !b.build_stack.is_empty() {
        const PB_OFFSET_X: f32 = 283.0;
        const PB_OFFSET_Y: f32 = 71.0;
        const PB_WIDTH: f32 = 87.0;
        let rect = [
            panel_x + PB_OFFSET_X * scale,
            panel_y + PB_OFFSET_Y * scale,
            PB_WIDTH * scale,
            bar_h_design * scale,
        ];

        // One-shot log so we can verify the rect + fill on first build.
        static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            log::debug!(
                "progress: pushing bar rect=({:.0},{:.0},{:.0},{:.0}) fill={:.2}",
                rect[0],
                rect[1],
                rect[2],
                rect[3],
                b.build_stack.progress(),
            );
        }

        state.progress_bars.push(ProgressBar {
            rect,
            fill: b.build_stack.progress(),
        });
    }
}

/// Port of `CInterface::LogicTakt`'s per-frame visibility dispatch
/// for the `if/Main` panel (CInterface.cpp:1214-1635). Reads
/// `player_side.curr_sel` + the currently-active building's kind /
/// stack state to decide which `if/Main` elements should show this
/// frame. Other panels don't have their dispatch ported yet; they
/// stay hidden.
/// Build the software cursor at init: parse the `Cursors` block,
/// preload the five sprite-sheet atlases, select the arrow
/// (MatrixGame.cpp:548) and hide the OS cursor over the window —
/// the port of `m_OldCursor = SetCursor(NULL)` in the CMatrixCursor
/// ctor.
fn init_matrix_cursor(
    matrix_data: &crate::matrix_lib::base::storage::Storage,
    iface_renderer: &mut crate::matrix_game::interface::InterfaceRenderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    read: &dyn Fn(&str) -> Option<Vec<u8>>,
    window: &Window,
) -> crate::matrix_game::cursor::MatrixCursor {
    let mut cursor = crate::matrix_game::cursor::MatrixCursor::new(matrix_data);
    let paths: Vec<String> = cursor.tex_paths().map(|s| s.to_string()).collect();
    for p in &paths {
        iface_renderer.ensure_atlas(device, queue, read, p);
    }
    cursor.select(crate::matrix_game::cursor::CURSOR_ARROW);
    window.set_cursor_visible(false);
    cursor
}

/// Per-frame mouse ray-trace — `m_TraceStopObj` / `m_TraceStopPos`
/// (MatrixMap.cpp:1149-1178). Skips the arcaded robot like the
/// original so the arcade reticle sees through the player's own hull.
fn update_mouse_trace(state: &mut AppState) {
    use crate::matrix_game::common::TRACE_ALL;
    use crate::matrix_game::map_trace::{trace, TraceStop};
    let [mx, my] = state.cursor;
    if mx < 0.0 {
        state.mouse_trace = TraceStop::None;
        return;
    }
    let w = state.gfx.config.width as f32;
    let h = state.gfx.config.height as f32;
    let (o, d) = state.camera.screen_to_world_ray(mx, my, w, h);
    let (stop, pos) = trace(
        &state.map,
        &state.game.objects,
        o,
        o + d * 10_000.0,
        TRACE_ALL,
        state.game.objects.arcaded_object,
    );
    state.mouse_trace = stop;
    state.mouse_trace_pos = pos;
}

/// `CMatrixMap::IsTraceNonPlayerObj` (MatrixMap.cpp:3630-3643) — a
/// robot / building / cannon / flyer / special object under the
/// cursor that isn't the player's.
fn is_trace_non_player_obj(state: &AppState) -> bool {
    use crate::matrix_game::common::PLAYER_SIDE;
    use crate::matrix_game::map_static::ObjectType;
    let Some(id) = state.mouse_trace.object() else {
        return false;
    };
    let Some(obj) = state.game.objects.get(id) else {
        return false;
    };
    (matches!(
        obj.core().obj_type,
        ObjectType::RobotAi | ObjectType::Building | ObjectType::Cannon | ObjectType::Flyer
    ) || obj.is_special())
        && obj.side() != PLAYER_SIDE
}

/// `CMatrixRobotAI::CheckFireDist` (MatrixRobot.cpp:5411-5433) for the
/// arcaded robot: true when any weapon's ray toward the cursor point,
/// bounded by that weapon's range, reaches the object under the cursor.
fn arcade_check_fire_dist(state: &AppState) -> bool {
    use crate::matrix_game::common::TRACE_ALL;
    use crate::matrix_game::map_static::MapStatic;
    use crate::matrix_game::map_trace::trace;
    let Some(aid) = state.game.objects.arcaded_object else {
        return false;
    };
    let Some(r) = crate::matrix_game::logic::robot_ref(&state.game.objects, aid) else {
        return false;
    };
    let point = state.mouse_trace_pos;
    for bw in &r.weapons {
        let Some(we) = state.game.objects.weapons.get(bw.weapon) else {
            continue; // `IsEffectPresent`
        };
        // `GetWeaponPos` — real muzzle when mounted, geo-center before
        // the first render frame (same fallback as weapons_logic_takt).
        let wpos = r
            .weapon_mounts
            .get(bw.unit)
            .copied()
            .flatten()
            .map(|m| m.pos)
            .unwrap_or(r.core().geo_center);
        let out = (point - wpos).normalize_or_zero() * we.weapon_dist();
        if out == glam::Vec3::ZERO {
            continue;
        }
        let (stop, _) = trace(
            &state.map,
            &state.game.objects,
            wpos,
            wpos + out,
            TRACE_ALL,
            Some(aid),
        );
        if stop == state.mouse_trace {
            return true;
        }
    }
    false
}

/// Context-sensitive gameplay cursor — faithful port of the "Cursor
/// logic" block of `CIFaceList::LogicTakt` (CInterface.cpp:2957-3010):
/// arcade reticle (red = in weapon range, yellow = enemy out of range,
/// blue = no target, arrow over the UI), screen-edge scroll star,
/// armed pre-order crosses, arrow otherwise.
fn update_cursor(state: &mut AppState) {
    use crate::matrix_game::common::PLAYER_SIDE;
    use crate::matrix_game::cursor::*;
    use crate::matrix_game::interface::iface_list::PreOrder;
    use crate::matrix_game::map_static::{MapStatic, ObjectType};

    let [mx, my] = state.cursor;
    let w = state.gfx.config.width as f32;
    let h = state.gfx.config.height as f32;

    // `ps->IsArcadeMode() && ps->GetArcadedObject()->IsLiveRobot()`.
    let arcaded_live = state
        .game
        .objects
        .arcaded_object
        .and_then(|id| crate::matrix_game::logic::robot_ref(&state.game.objects, id))
        .map(|r| r.is_live())
        .unwrap_or(false);

    let name = if arcaded_live {
        if mx >= 0.0 && state.iface_list.hit_test(mx, my, w, h).is_some() {
            CURSOR_ARROW
        } else if is_trace_non_player_obj(state) {
            if arcade_check_fire_dist(state) {
                CURSOR_CROSS_RED
            } else {
                CURSOR_CROSS_YELLOW
            }
        } else {
            CURSOR_CROSS_BLUE
        }
    } else {
        // MatrixFormGame.hpp:9 — the same 4 px band the edge-scroll uses.
        const MOUSE_BORDER: f32 = 4.0;
        let in_screen = mx >= 0.0 && my >= 0.0 && mx < w && my < h;
        if in_screen
            && (mx < MOUSE_BORDER
                || mx > w - MOUSE_BORDER
                || my < MOUSE_BORDER
                || my > h - MOUSE_BORDER)
        {
            CURSOR_STAR
        } else {
            match state.iface_list.pre_order {
                Some(PreOrder::Fire) | Some(PreOrder::Bomb) => {
                    if is_trace_non_player_obj(state) {
                        CURSOR_CROSS_RED
                    } else {
                        CURSOR_CROSS_BLUE
                    }
                }
                Some(PreOrder::Capture) => {
                    let enemy_building = state
                        .mouse_trace
                        .object()
                        .and_then(|id| state.game.objects.get(id))
                        .map(|o| {
                            o.core().obj_type == ObjectType::Building && o.side() != PLAYER_SIDE
                        })
                        .unwrap_or(false);
                    if enemy_building {
                        CURSOR_CROSS_RED
                    } else {
                        CURSOR_CROSS_BLUE
                    }
                }
                Some(PreOrder::Move) | Some(PreOrder::Patrol) | Some(PreOrder::Repair) => {
                    CURSOR_CROSS_BLUE
                }
                None => CURSOR_ARROW,
            }
        }
    };
    state.matrix_cursor.select(name);
}

fn refresh_interface_visibility(state: &mut AppState) {
    use crate::matrix_game::interface::MainVisibilityCtx;
    use crate::matrix_game::map_static::{MapStatic, ObjectType};
    use crate::matrix_game::object_building::{Building, BuildingType};
    use crate::matrix_game::side::CurrSel;

    update_cursor(state);
    let curr_sel = state.game.player_side.curr_sel;
    let player_side_id = state.game.player_side.id;
    // Pull building context when the selection is a Building. Also
    // carry the selected building's `m_Side` so the Base-panel build
    // button can gate on "is this our base?" — port of the
    // `m_Base->m_Side == PLAYER_SIDE` guard at CConstructor.cpp:225-227.
    use crate::matrix_game::interface::constructor::RobotConfig;
    use crate::matrix_game::interface::MAX_STACK_ICONS;

    // First pass — read-only snapshot of the build stack: per-slot
    // turret kinds and per-slot robot configs. We can't borrow
    // `state.game.objects` and `state.robot_icons`/`state.iface_renderer`
    // mutably at the same time, so the icon-bake pass runs after the
    // borrow is released.
    let (
        kind,
        stack_empty,
        stack_items,
        turrets_max,
        hit_point,
        hit_point_max,
        active_side,
        stack_kinds,
        stack_robot_cfgs,
    ): (
        Option<BuildingType>,
        bool,
        i32,
        i32,
        f32,
        f32,
        i32,
        [Option<i32>; MAX_STACK_ICONS],
        [Option<RobotConfig>; MAX_STACK_ICONS],
    ) = match curr_sel {
        CurrSel::BaseSelected | CurrSel::BuildingSelected => {
            use crate::matrix_game::object_building::PendingKind;
            let active = state.game.active_object();
            active
                .and_then(|id| state.game.objects.get(id))
                .filter(|o| matches!(o.core().obj_type, ObjectType::Building))
                .map(|o| {
                    let b: &Building = unsafe { &*(o as *const dyn MapStatic as *const Building) };
                    let n = b.build_stack.items() as i32;
                    let mut kinds: [Option<i32>; MAX_STACK_ICONS] = [None; MAX_STACK_ICONS];
                    let mut cfgs: [Option<RobotConfig>; MAX_STACK_ICONS] = [None; MAX_STACK_ICONS];
                    for (slot, item) in b
                        .build_stack
                        .list()
                        .iter()
                        .take(MAX_STACK_ICONS)
                        .enumerate()
                    {
                        match item.kind {
                            PendingKind::Turret { turret_kind, .. } => {
                                kinds[slot] = Some(turret_kind)
                            }
                            PendingKind::Robot(cfg) => cfgs[slot] = Some(cfg),
                        }
                    }
                    (
                        Some(b.kind),
                        n == 0,
                        n,
                        b.turrets_max,
                        b.hit_point,
                        b.hit_point_max,
                        b.side,
                        kinds,
                        cfgs,
                    )
                })
                .unwrap_or((
                    None,
                    true,
                    0,
                    0,
                    0.0,
                    0.0,
                    0,
                    [None; MAX_STACK_ICONS],
                    [None; MAX_STACK_ICONS],
                ))
        }
        _ => (
            None,
            true,
            0,
            0,
            0.0,
            0.0,
            0,
            [None; MAX_STACK_ICONS],
            [None; MAX_STACK_ICONS],
        ),
    };

    // Bake (or look up cached) robot icons for any robot configs
    // queued in the build stack. Each new config triggers a separate
    // off-screen submit, after which the texture is reused for free.
    let mut stack_robot_keys: [Option<String>; MAX_STACK_ICONS] = Default::default();
    if let Some(robots) = state.terrain.robots() {
        let format = state.gfx.config.format;
        for (slot, cfg_opt) in stack_robot_cfgs.iter().enumerate() {
            if let Some(cfg) = cfg_opt {
                stack_robot_keys[slot] = state.robot_icons.ensure(
                    &state.gfx.device,
                    &state.gfx.queue,
                    format,
                    robots,
                    &mut state.iface_renderer,
                    cfg,
                );
            }
        }
    }
    let active_is_player_owned = active_side == player_side_id;

    // Port of `CMatrixSideUnit::GetIncomePerTime(kind, 60000)`
    // (MatrixSide.cpp:352-377). The original's per-ms scaling is
    // commented out, so this returns the flat per-tick rate:
    // `RESOURCES_INCOME_BASE * fu / 100` for a base (3 @ fu=100),
    // `RESOURCES_INCOME` for a factory (10).
    const RESOURCES_INCOME: i32 = 10;
    const RESOURCES_INCOME_BASE: i32 = 3;
    // `GetResourceForceUp()` — the map's SideResInfo income multiplier.
    let force_up = state.game.player_side.get_resource_force_up();
    let income_per_minute = match kind {
        Some(BuildingType::Base) => RESOURCES_INCOME_BASE * force_up / 100,
        Some(_) => RESOURCES_INCOME,
        None => 0,
    };

    let constructor_active = state
        .game
        .player_side
        .builder
        .as_ref()
        .map(|b| b.active)
        .unwrap_or(false);
    let turret_build_active = state.iface_list.turret_build.is_active();
    // `kind.is_some()` ↔ player has clicked tur1..tur4 and we're now in
    // the placement preview (m_CurrentAction == BUILDING_TURRET in C++).
    // CInterface.cpp:1710 hides the picker buttons in that sub-mode.
    let turret_kind_committed = state.iface_list.turret_build.kind.is_some();

    // Per-kind picker DISABLED + buca DISABLED — port of
    // CInterface.cpp:1602-1605 (`buca`) and 1713-1738 (`tur1..4`):
    // a button is DISABLED when the building has no free turret slot
    // OR the player can't afford the cannon. `buca` is also disabled
    // when the build stack is full.
    //
    // Also collect per-slot installed `cannon_type` so the Main panel
    // can overlay `bt{N}` icons on the `podl{N}` strip — port of
    // `CIFaceList::CreateDynamicTurrets` (CInterface.cpp:4572-4621).
    let mut turret_disabled = [false; 4];
    let mut buca_disabled = false;
    let mut installed_turret_kinds: [Option<i32>; 4] = [None; 4];
    if matches!(curr_sel, CurrSel::BaseSelected | CurrSel::BuildingSelected) {
        if let Some(active_id) = state.game.active_object() {
            if let Some(o) = state.game.objects.get(active_id) {
                if matches!(o.core().obj_type, ObjectType::Building) {
                    let b: &Building = unsafe { &*(o as *const dyn MapStatic as *const Building) };
                    let has_free_slot = b.turret_places.iter().any(|p| p.cannon_type < 0);
                    let stack_full = b.build_stack.is_full();
                    let cfg = crate::matrix_game::config::global();
                    let side = &state.game.player_side;
                    let mut affordable = [false; 4];
                    for k in 0..4 {
                        let cost = cfg.turrets.cost_of((k + 1) as i32);
                        affordable[k] = crate::matrix_game::config::Resource::ALL
                            .iter()
                            .all(|r| side.get_resource_amount(*r) >= cost.resources[*r as usize]);
                        turret_disabled[k] = !has_free_slot || !affordable[k];
                    }
                    let any_affordable = affordable.iter().any(|a| *a);
                    buca_disabled = !has_free_slot || stack_full || !any_affordable;
                    for (i, p) in b.turret_places.iter().take(4).enumerate() {
                        if p.cannon_type > 0 {
                            installed_turret_kinds[i] = Some(p.cannon_type);
                        }
                    }
                }
            }
        }
    }

    // Selected-robot panel snapshot. Port of the work_group walk at
    // CInterface.cpp:1227-1311 + CreatePersonal/CreateGroupIcons inputs
    // — for each selected robot (own side or enemy / single or group),
    // collect the cached `RobotConfig` and HP readout, plus the
    // `m_CurrSelNum` index pointing at the active member.
    let mut robot_panel: Option<crate::matrix_game::interface::RobotPanelCtx> = None;
    if matches!(curr_sel, CurrSel::RobotsSelected) {
        use crate::matrix_game::interface::{RobotEntry, RobotPanelCtx};
        use crate::matrix_game::map_static::ObjectId as OId;
        use crate::matrix_game::robot::Robot;

        // Snapshot the selected ids + their cfg/HP/name. Read the
        // robot pointer through MapStatic and downcast — the same
        // cast pattern the marquee/selection code uses elsewhere.
        let primary = state.game.player_side.active_object;
        let ids: Vec<OId> = state.game.player_side.selected.clone();
        let mut snapshot: Vec<(
            OId,
            crate::matrix_game::interface::constructor::RobotConfig,
            String,
            i32,
            i32,
        )> = Vec::with_capacity(ids.len());
        for id in &ids {
            let Some(obj) = state.game.objects.get(*id) else {
                continue;
            };
            if !matches!(obj.core().obj_type, ObjectType::RobotAi) {
                continue;
            }
            let r: &Robot = unsafe { &*(obj as *const dyn MapStatic as *const Robot) };
            // `CMatrixRobotAI::GetHitPoint()` returns `m_HitPoint/10`
            // (MatrixObjectRobot.hpp:243-244) — the lives caption shows
            // the /10 value, same scale as the building readout.
            let hp = (r.hit_point / 10.0).round().max(0.0) as i32;
            let max_hp = (r.hit_point_max / 10.0).round().max(0.0) as i32;
            snapshot.push((*id, r.config, r.name.clone(), hp, max_hp));
        }
        if !snapshot.is_empty() {
            // Bake the medium icons (group cells) ahead of time. Each
            // unique config maps to a single 64×64 atlas key.
            let format = state.gfx.config.format;
            let mut group: Vec<RobotEntry> = Vec::with_capacity(snapshot.len());
            if let Some(robots) = state.terrain.robots() {
                for (_id, cfg, _name, _hp, _max) in &snapshot {
                    let key = state.robot_icons.ensure(
                        &state.gfx.device,
                        &state.gfx.queue,
                        format,
                        robots,
                        &mut state.iface_renderer,
                        cfg,
                    );
                    group.push(RobotEntry { atlas_key: key });
                }
            } else {
                for _ in &snapshot {
                    group.push(RobotEntry { atlas_key: None });
                }
            }
            // Selected index: position of `active_object` within the
            // snapshot. `m_CurrSelNum` in C++ — the C++ panel uses it
            // to draw the ramka over the active slot and to pick the
            // robot whose big-portrait + name + HP appear on the left
            // of the panel.
            let selected_index = primary
                .and_then(|p| snapshot.iter().position(|(id, _, _, _, _)| *id == p))
                .unwrap_or(0);
            let (_id, cur_cfg, cur_name, cur_hp, cur_max) = &snapshot[selected_index];
            // Bake / look up the big portrait for the active robot.
            let personal_atlas_key = state.terrain.robots().and_then(|robots| {
                state.robot_icons.ensure_big(
                    &state.gfx.device,
                    &state.gfx.queue,
                    format,
                    robots,
                    &mut state.iface_renderer,
                    cur_cfg,
                )
            });
            // SINGLE_MODE weapon readout — one robot selected only
            // (CreateWeaponDynamicStatics, CInterface.cpp:3013-3114).
            let single_weapons: Vec<(usize, i32, f32)> = if snapshot.len() == 1 {
                let sid = snapshot[0].0;
                crate::matrix_game::logic::robot_ref(&state.game.objects, sid)
                    .map(|r| {
                        r.weapons
                            .iter()
                            .filter_map(|bw| {
                                let kind = r.config.weapon.get(bw.unit)?.kind.0;
                                if kind == 0 {
                                    return None;
                                }
                                // alpha = heat·0.25 as a byte (:1526-1535).
                                let a = (bw.heat as f32 * 0.25 / 255.0).clamp(0.0, 1.0);
                                Some((bw.unit, kind, a))
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            robot_panel = Some(RobotPanelCtx {
                group,
                selected_index,
                personal_atlas_key,
                robot_name: cur_name.clone(),
                robot_hp: *cur_hp,
                robot_max_hp: *cur_max,
                single_weapons,
            });
        }
    }

    // An armed order only makes sense while robots are selected —
    // selection loss (death / deselect) backs out of ordering mode.
    if state.iface_list.pre_order.is_some() && !matches!(curr_sel, CurrSel::RobotsSelected) {
        state.iface_list.pre_order = None;
    }
    // Active-robot weapon check for the obomb/orep buttons
    // (bomber_sel/repairer_sel, CInterface.cpp:1302-1305).
    let (has_bomber, has_repairer) = match state
        .game
        .player_side
        .get_cur_sel_object()
        .and_then(|id| crate::matrix_game::logic::robot_ref(&state.game.objects, id))
    {
        Some(r) => (
            r.have_bomb(&state.game.objects),
            r.have_repair(&state.game.objects),
        ),
        None => (false, false),
    };
    // Auto-order toggle state for the current group (AUTO_*_ON).
    let (auto_cap, auto_atk, auto_def) = state.game.show_order_state();
    // Order-glow index for the active group's current manual order.
    // Order glow flags aggregated across ALL selected robots
    // (CInterface.cpp:1316-1354) with the cross-suppression rules at
    // :1663-1677: move suppressed by capture/bomb/repair; bomb/repair
    // glow needs a bomber/repairer in the selection.
    let order_glows = if curr_sel == CurrSel::RobotsSelected {
        use crate::matrix_game::side::PlayerOrder;
        let mut flags = [false; 6]; // stop/move/patrol/fire/capt/bomb|rep
        let (mut capt_like, mut bomb_or_rep, mut bomber_or_rep_sel) = (false, false, false);
        for &rid in &state.game.player_side.selected {
            let Some(r) = crate::matrix_game::logic::robot_ref(&state.game.objects, rid) else {
                continue;
            };
            if r.have_bomb(&state.game.objects) || r.have_repair != 0 {
                bomber_or_rep_sel = true;
            }
            if r.group_logic < 0 {
                continue;
            }
            let Some(g) = state
                .game
                .player_side
                .player_groups
                .get(r.group_logic as usize)
            else {
                continue;
            };
            match g.order {
                PlayerOrder::Stop => flags[0] = true,
                PlayerOrder::MoveTo => flags[1] = true,
                PlayerOrder::Patrol => flags[2] = true,
                PlayerOrder::Attack => flags[3] = true,
                PlayerOrder::Capture => {
                    flags[4] = true;
                    capt_like = true;
                }
                PlayerOrder::Bomb | PlayerOrder::Repair => bomb_or_rep = true,
                _ => {}
            }
        }
        flags[1] = flags[1] && !capt_like && !bomb_or_rep;
        flags[5] = bomb_or_rep && bomber_or_rep_sel;
        flags
    } else {
        [false; 6]
    };
    let ctx = MainVisibilityCtx {
        curr_sel,
        building_kind: kind,
        building_stack_empty: stack_empty,
        building_stack_items: stack_items,
        building_turrets_max: turrets_max,
        constructor_active,
        ordering: state.iface_list.pre_order.is_some(),
        turret_build_active,
        turret_kind_committed,
        turret_disabled,
        buca_disabled,
        maintenance_cooling: state.game.maintenance_disabled() || state.game.maintenance_time > 0,
        maintenance_t: if state.game.maintenance_disabled() {
            0.0
        } else {
            // `BeforMaintenanceTimeT` (MatrixMap.hpp:471).
            let cfg = crate::matrix_game::config::global();
            let total = cfg.difficulty.k_time_before_maintenance
                * (cfg.timings.maintenance_period * state.game.maintenance_prc / 100) as f32;
            1.0 - state.game.maintenance_time as f32 / total.max(1.0)
        },
        installed_turret_kinds,
        building_stack_turret_kinds: stack_kinds,
        building_stack_robot_atlas_keys: stack_robot_keys,
        robot_panel: robot_panel.clone(),
        has_bomber,
        has_repairer,
        auto_frobot_on: auto_atk,
        auto_protect_on: auto_def,
        auto_capture_on: auto_cap,
        order_glows,
        arcade_mode: state.game.is_arcade_mode(),
        single_disable_manual: state
            .game
            .player_side
            .get_cur_sel_object()
            .and_then(|id| crate::matrix_game::logic::robot_ref(&state.game.objects, id))
            .map(|r| r.is_disable_manual())
            .unwrap_or(false),
    };
    if let Some(p) = state.iface_list.panel_mut("Main") {
        p.refresh_main_visibility(&ctx);
        // Push per-kind captions + HP readout into the dynamic labels
        // — ports CInterface.cpp:1369-1499 (name / bopis / bresg / lives).
        if let Some(k) = kind {
            let strings = crate::matrix_game::config::global_strings();
            p.apply_main_building_text(
                k,
                hit_point,
                hit_point_max,
                income_per_minute,
                &strings.buildings,
            );
        }
        // Robot-selection captions — port of CInterface.cpp:1369-1404.
        if let Some(rp) = robot_panel.as_ref() {
            p.apply_main_robot_text(&rp.robot_name, rp.robot_hp, rp.robot_max_hp);
        }
    }
    // Top-of-screen permanent HUD — resource pools + robot count. Port
    // of the CInterface::AddHintReplacements-driven `thz/enhz1/elhz/
    // phz/rvhz` substitution at CInterface.cpp:4444-4462, applied to
    // the Top panel's always-visible value labels.
    {
        use crate::matrix_game::config::Resource;
        let side = &state.game.player_side;
        let titan = side.get_resource_amount(Resource::Titan);
        let elect = side.get_resource_amount(Resource::Electronics);
        let energy = side.get_resource_amount(Resource::Energy);
        let plasma = side.get_resource_amount(Resource::Plasma);
        let robots = side.get_side_robots();
        let max_robots = state.game.compute_max_side_robots(side.id);
        if let Some(p) = state.iface_list.panel_mut("Top") {
            p.apply_top_hud_text(titan, elect, energy, plasma, robots, max_robots);
        }
    }
    // Snapshot the live preset so we can feed it to `apply_constructor_to_pylons`
    // without holding a borrow across the panel_mut call.
    let live_cfg = state.game.player_side.builder.as_ref().map(|b| *b.cfg());
    let hist_prev = state.iface_list.history.is_prev();
    let hist_next = state.iface_list.history.is_next();
    let build_count = state.iface_list.r_count_control.counter();
    let counter_state = state.iface_list.r_count_control.clone();
    let counter_ctx = build_counter_ctx(state);
    let (focused_price, summ_price, armor_common, armor_extra, build_enabled, focused_target) =
        state
            .game
            .player_side
            .builder
            .as_ref()
            .map(|b| {
                // Armor weapon-slot caps from the live preview's matrix
                // (port of g_MatrixMap->m_RobotWeaponMatrix[hull-1] reads).
                let common = b.live_armor.max_common_weapon_cnt;
                let extra = b.live_armor.max_extra_weapon_cnt;
                // Port of `g_IFaceList->CreateSummPrice(m_Counter)`
                // (CInterface.cpp:3220 / CCounter.cpp:42-50). The C++
                // multiplies per-unit price by the counter at panel-refresh
                // time; we do the same here.
                let mult = build_count.max(1);
                let mut total_cost = b.construction_price();
                for r in 0..total_cost.resources.len() {
                    total_cost.resources[r] = total_cost.resources[r].saturating_mul(mult);
                }
                // Build button enabled: stack not full + side can afford
                // the live preview cost. Ports CInterface.cpp:1850-1867.
                let mut enough = true;
                for r in crate::matrix_game::config::Resource::ALL {
                    if state.game.player_side.get_resource_amount(r)
                        < total_cost.resources[r as usize]
                    {
                        enough = false;
                        break;
                    }
                }
                let buildable_base = matches!(kind, Some(BuildingType::Base));
                let under_cap = counter_ctx.side_robots + counter_ctx.robots_in_stack
                    < counter_ctx.max_side_robots;
                // Stack-full disable (CInterface.cpp:1815-1817:
                // `bld = m_BS.GetItemsCnt() < MAX_STACK_UNITS`).
                let stack_full = state
                    .game
                    .active_object()
                    .and_then(|id| {
                        crate::matrix_game::logic::building_ref(&state.game.objects, id)
                    })
                    .map(|bb| bb.build_stack.is_full())
                    .unwrap_or(false);
                // Port of `CConstructor::RemoteBuild`'s player-side guard
                // at CConstructor.cpp:225-227 — enemy / neutral bases can
                // be selected for inspection but the build button must not
                // fire on them.
                enough =
                    enough && buildable_base && under_cap && active_is_player_owned && !stack_full;
                // Decode the focused pylon → (type, kind) so the visibility
                // refresh knows which `head{N}_st` / `iw{N}text` etc. to
                // expose.
                let focused = b
                    .focused_element
                    .as_deref()
                    .and_then(|n| b.focus_target_for(n));
                (b.focused_price, total_cost, common, extra, enough, focused)
            })
            .unwrap_or((
                None,
                crate::matrix_game::interface::constructor::UnitPrice::zero(),
                0,
                0,
                true,
                None,
            ));
    // Robot-limit warning gate — port of CInterface.cpp:1830-1831
    // (`ps->GetRobotsCnt()+ps->GetRobotsInStack() >= ps->GetMaxSideRobots()`).
    let robot_limit_reached =
        counter_ctx.side_robots + counter_ctx.robots_in_stack >= counter_ctx.max_side_robots;
    // Per-pylon CRITICAL_RAMKA (CInterface.cpp:1885-2097): the total
    // (×counter) price is short on a resource that the MOUNTED part
    // actually costs. Empty pylons stay NORMAL (Param2 == 0 there).
    let pylon_criticals: [bool; 8] = {
        use crate::matrix_game::config::Resource;
        use crate::matrix_game::interface::constructor::UnitPrice;
        let mut out = [false; 8];
        if let Some(cfg) = live_cfg.as_ref() {
            let prices = crate::matrix_game::config::global().prices;
            let short: [bool; 4] = {
                let mut s = [false; 4];
                for r in Resource::ALL {
                    s[r as usize] = state.game.player_side.get_resource_amount(r)
                        < summ_price.resources[r as usize];
                }
                s
            };
            let crit = |p: UnitPrice| -> bool {
                (0..4).any(|i| short[i] && p.resources[i] != 0)
            };
            if !cfg.head.is_empty() {
                out[0] = crit(prices.get(cfg.head.ty, cfg.head.kind));
            }
            if !cfg.hull.unit.is_empty() {
                out[1] = crit(prices.get(cfg.hull.unit.ty, cfg.hull.unit.kind));
            }
            if !cfg.chassis.is_empty() {
                out[2] = crit(prices.get(cfg.chassis.ty, cfg.chassis.kind));
            }
            for (i, w) in cfg.weapon.iter().enumerate().take(5) {
                if !w.is_empty() {
                    out[3 + i] = crit(prices.get(w.ty, w.kind));
                }
            }
        }
        out
    };
    // Port of the C++ `m_VisibleAlpha = IS_VISIBLEA` gate at
    // CInterface.cpp:1797 — the Base panel is *visible* whenever a
    // base/building is the active selection; whether individual
    // constructor elements inside render is gated on
    // `constructor_active` via the per-element refresh below.
    let base_panel_visible =
        matches!(curr_sel, CurrSel::BaseSelected | CurrSel::BuildingSelected) && kind.is_some();
    if let Some(p) = state.iface_list.panel_mut("Base") {
        let was_visible = p.visible;
        p.visible = base_panel_visible;
        p.refresh_base_visibility_v2(
            crate::matrix_game::interface::interface::BaseVisibilityCtx {
                constructor_active,
                build_count,
                focused_price: focused_price.as_ref(),
                summ_price: &summ_price,
                armor_common_slots: armor_common,
                armor_extra_slots: armor_extra,
                history_has_prev: hist_prev,
                history_has_next: hist_next,
                counter_up_enabled: counter_state.button_up_enabled,
                counter_down_enabled: counter_state.button_down_enabled,
                build_enabled,
                robot_limit_reached,
                focused_target,
                pylon_criticals,
            },
        );
        if constructor_active {
            if let Some(cfg) = live_cfg.as_ref() {
                p.apply_constructor_to_pylons(cfg);
            }
            // Push the focused-component label / description into the
            // `it_label1` / `it_label2` statics + the live preview's
            // structure / damage / robot-name into their respective
            // statics. Matches CInterface.cpp:1822-1827 (rcname),
            // 1869-1884 (it_label1/2), and 2330-2354 (struct/damage).
            if let Some(b) = state.game.player_side.builder.as_ref() {
                let structure = b.construction_structure();
                // C++ damage path zeroes the value when there are no
                // weapons (CInterface.cpp:2347-2350); construction_damage
                // already sums only populated weapon slots, so the same
                // outcome falls out.
                let damage = b.construction_damage();
                let robot_name = b.construction_name();
                p.apply_focused_text(
                    &b.focused_text.label,
                    &b.focused_text.description,
                    structure,
                    damage,
                    &robot_name,
                    &summ_price,
                    b.focused_price.as_ref(),
                );
            }
        }
        // Per-frame port of CIFaceList::CreateSummPrice / CreateItemPrice
        // (CInterface.cpp:3146-3297). Wipes the prior frame's dynamic
        // price icons + price-text labels, then re-emits them for every
        // non-zero resource on the current focused / summary price.
        let focused_price_owned = state
            .game
            .player_side
            .builder
            .as_ref()
            .and_then(|b| b.focused_price);
        let side_res = state.game.player_side.resources;
        p.apply_constructor_prices_res(
            constructor_active,
            focused_price_owned.as_ref(),
            &summ_price,
            &side_res,
        );
        if was_visible != p.visible {
            let n_vis = p.elements.iter().filter(|e| e.visible()).count();
            let n_total = p.elements.len();
            let visible_names: Vec<&str> = p
                .elements
                .iter()
                .filter(|e| e.visible())
                .map(|e| e.name.as_str())
                .take(20)
                .collect();
            log::debug!(
                "iface: Base panel visibility → {} (constructor_active={}, {}/{} elements visible, first: {:?})",
                p.visible,
                constructor_active,
                n_vis,
                n_total,
                visible_names,
            );
        }
    }
}

/// Keep the selection-ring effects in sync with
/// `player_side.selected`. Called once per frame after the logic
/// takt advances. Ports the effect-lifecycle hooks
/// `CMatrixBuilding::Select` / `UnSelect` +
/// `CMatrixEffectSelection::SetPos` (MatrixObjectBuilding.cpp:1460-1525,
/// MatrixEffectSelection.hpp:61-65) as a single reconciler, driven
/// off the whole multi-select set so every selected object gets its
/// own ring.
fn sync_selection_ring(state: &mut AppState, step_ms: f32) {
    use crate::matrix_game::map_static::{ObjectId, ObjectType};
    use crate::matrix_game::object_building::{selection_placement, Building, BuildingType};

    // Every C++ CreateSelection call site uses the default green — the
    // SetColor(SEL_COLOR_TMP) calls at MatrixSide.cpp:1750-1753 are
    // commented out in the shipping code.
    let color = crate::matrix_game::effects::selection::SEL_COLOR_DEFAULT;
    let mut desired: Vec<(ObjectId, glam::Vec3, f32, u32)> = Vec::new();
    let ids: Vec<ObjectId> = state.game.player_side.selected.clone();
    for id in ids {
        if !state.game.objects.is_valid(id) {
            continue;
        }
        let placement = state.game.objects.get(id).map(|obj| {
            let ot = obj.core().obj_type;
            if ot == ObjectType::Building {
                let b: &Building = unsafe {
                    &*(obj as *const dyn crate::matrix_game::map_static::MapStatic
                        as *const Building)
                };
                let (c, r) = selection_placement(b.pos, b.build_z, b.angle, b.kind);
                let z = state.map.get_z(c.x, c.y).max(c.z);
                let _ = BuildingType::Base; // keep import live
                (glam::Vec3::new(c.x, c.y, z), r)
            } else {
                // Fixed ring size — ROBOT_SELECTION_SIZE / FLYER_SELECTION_SIZE
                // are both 20 in the original (MatrixRobot.hpp:42,
                // MatrixFlyer.hpp:51), independent of collision radius.
                // Robots anchor at body-matrix z + 3 (MatrixRobot.cpp:
                // 5280/5299) — geo_center sits ~9 above the ground and
                // floats the ring visibly high.
                let gc = obj.core().geo_center;
                let c = if matches!(
                    obj.core().obj_type,
                    crate::matrix_game::map_static::ObjectType::RobotAi
                ) {
                    let r: &crate::matrix_game::robot::Robot = unsafe {
                        &*(obj as *const dyn crate::matrix_game::map_static::MapStatic
                            as *const crate::matrix_game::robot::Robot)
                    };
                    glam::Vec3::new(gc.x, gc.y, r.pos_z + 3.0)
                } else {
                    gc
                };
                (c, 20.0)
            }
        });
        if let Some((c, r)) = placement {
            desired.push((id, c, r, color));
        }
    }
    state.selection_ring.set_selections(&desired);
    state
        .selection_ring
        .takt(step_ms, |x, y| state.map.get_z(x, y));
}

#[cfg(not(target_arch = "wasm32"))]
fn load_map() -> (
    GameMap,
    crate::matrix_lib::base::storage::Storage,
    crate::matrix_lib::base::storage::Storage,
    crate::matrix_lib::base::pack::PkgArchive,
) {
    use crate::matrix_lib::base::pack::PkgArchive;
    use crate::matrix_lib::base::storage::Storage;

    let candidates = [
        "../Data/robots.pkg",
        "Data/robots.pkg",
        "../../Data/robots.pkg",
    ];
    let host_pkg = std::env::var("MG_PKG").ok();
    let pkg_path = host_pkg.as_deref().unwrap_or_else(|| {
        candidates
            .iter()
            .find(|c| std::path::Path::new(c).exists())
            .unwrap_or(&"../Data/robots.pkg")
    });

    log::info!("loading pkg: {}", pkg_path);
    let pkg_data = std::fs::read(pkg_path).expect("failed to read robots.pkg");
    let pkg = PkgArchive::from_bytes(pkg_data).expect("failed to parse pkg");

    // robots.dat is required, not optional — the original game crashes with
    // a null deref if it's absent (see MatrixGame.cpp:240-257, 371-373).
    let dat_candidates = [
        "../Data/robots.dat",
        "Data/robots.dat",
        "../../Data/robots.dat",
    ];
    let host_dat = std::env::var("MG_DAT").ok();
    let dat_path = host_dat.as_deref().unwrap_or_else(|| {
        dat_candidates
            .iter()
            .find(|c| std::path::Path::new(c).exists())
            .expect("robots.dat is required; place it next to robots.pkg (e.g. ../Data/robots.dat)")
    });
    log::info!("loading matrix data: {}", dat_path);
    let dat_bytes = std::fs::read(dat_path).expect("failed to read robots.dat");
    let matrix_data = Storage::from_bytes(&dat_bytes).expect("failed to parse robots.dat CStorage");

    // Pick the map the same way the original does (MatrixGame.cpp:383-394):
    // CLI arg wins; otherwise read `Config/Map` from the global data.
    let cli_map = std::env::args().nth(1);
    let mapname = match cli_map.as_deref() {
        Some(arg) if !arg.is_empty() => resolve_map_name(arg),
        _ => {
            let config_rec = matrix_data
                .block_record("da", "Config")
                .expect("robots.dat has no Config block");
            let map_param = matrix_data
                .block_param(&config_rec, "Map")
                .filter(|s| !s.is_empty())
                .expect(
                    "no map requested and Config/Map is missing — pass a map name as the first CLI arg",
                );
            resolve_map_name(&map_param)
        }
    };
    log::info!("loading map: {}", mapname);

    let cmap_data = pkg
        .read_file(&mapname)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", mapname, e));
    let stor = Storage::from_bytes(&cmap_data).expect("failed to parse map CStorage");
    let map = GameMap::from_cmap_bytes(&cmap_data).expect("failed to parse CMAP");
    crate::matrix_game::map::set_map_name(&mapname);

    (map, stor, matrix_data, pkg)
}

/// Mirror the path-building in `MatrixGame::Init` (MatrixGame.cpp:385-394):
/// a bare name like `Atoll` becomes `Matrix\Map\Atoll.CMAP`; anything with
/// a path separator is used verbatim. We normalize to forward slashes,
/// uppercase for the pkg's case-folded lookup, and append `.CMAP` when the
/// caller didn't supply it.
#[cfg(not(target_arch = "wasm32"))]
fn resolve_map_name(requested: &str) -> String {
    let trimmed = requested.trim();
    let has_sep = trimmed.contains('/') || trimmed.contains('\\');
    let normalized = trimmed.replace('\\', "/");
    let with_prefix = if has_sep {
        normalized
    } else {
        format!("MATRIX/MAP/{normalized}")
    };
    let with_ext = if with_prefix.to_uppercase().ends_with(".CMAP") {
        with_prefix
    } else {
        format!("{with_prefix}.CMAP")
    };
    with_ext.to_uppercase()
}

/// WASM counterpart of `load_map`. The bundle URL is taken from the page's
/// `?bundle=<url>` query parameter so the same build can serve different
/// maps without rewiring JS — this mirrors the original's configurable map
/// path (MatrixGame.cpp:383-394) inside the bundle-delivery model.
/// `assets/atoll.bundle` is kept as the default for backwards compatibility.
#[cfg(target_arch = "wasm32")]
async fn load_map_async() -> (
    GameMap,
    crate::matrix_lib::base::storage::Storage,
    crate::matrix_lib::base::storage::Storage,
    crate::gfx::bundle::AssetBundle,
) {
    use crate::gfx::bundle::AssetBundle;
    use crate::matrix_lib::base::storage::Storage;

    let bundle_url = bundle_url_from_query().unwrap_or_else(|| "assets/atoll.bundle".to_string());
    // Cache-bust: append the same `?v=N` that index.html uses for
    // the JS import so the browser refetches when the UI textures
    // in the bundle change.
    let bundle_url = if bundle_url.contains('?') {
        bundle_url
    } else {
        // Bump this whenever `pack_bundle.rs` changes the set of
        // packed keys, so the browser refetches instead of serving
        // a stale cached response.
        format!("{bundle_url}?bv=11")
    };
    log::info!("loading bundle: {}", bundle_url);
    let bundle_data = crate::gfx::loader::load_bytes(&bundle_url)
        .await
        .unwrap_or_else(|_| panic!("failed to fetch asset bundle: {}", bundle_url));
    let bundle = AssetBundle::from_bytes(&bundle_data).expect("failed to parse bundle");

    let cmap_data = bundle
        .read_file("map.cmap")
        .expect("no map.cmap in bundle")
        .to_vec();
    let stor = Storage::from_bytes(&cmap_data).expect("failed to parse CStorage");
    let map = GameMap::from_cmap_bytes(&cmap_data).expect("failed to parse CMAP");
    // The bundle URL stands in for the cmap path — briefing lookup
    // derives the map stem from it (see `briefing_stem`).
    crate::matrix_game::map::set_map_name(&bundle_url);

    // robots.dat is required here too — see the native loader comment.
    let dat_bytes = bundle
        .read_file("robots.dat")
        .expect("bundle must contain robots.dat; rebuild the bundle with examples/pack_bundle.rs");
    let matrix_data =
        Storage::from_bytes(dat_bytes).expect("failed to parse bundled robots.dat CStorage");
    log::info!("loaded matrix data from bundle (robots.dat)");

    (map, stor, matrix_data, bundle)
}

/// Drain all queued sound events into the mixer — the app-loop side
/// of the split: game logic and UI code queue
/// [`SndEvent`]s, this pump feeds them to the `CSound` port with the
/// current camera listener. Runs every frame, pause included.
fn pump_sounds(state: &mut AppState) {
    use crate::matrix_game::sound::{Interrupt, SndEvent, SndHandle};
    // `GetFrustumCenter()` / `GetRight()` — the CalcPanVol inputs.
    state
        .sounds
        .set_listener(state.camera.eye_pos_world(), state.camera.camera_right_world());
    // Weapon slots freed this takt — stop their looped hums (the C++
    // weapon destructor's FireEnd → StopPlay path).
    for id in std::mem::take(&mut state.game.objects.weapons.freed) {
        state.sounds.dispatch(SndEvent::Stop {
            handle: SndHandle::Weapon(id),
        });
    }
    for ev in state.game.objects.pending_sounds.drain(..) {
        state.sounds.dispatch(ev);
    }
    for gs in state.game.sound_queue.drain(..) {
        state.sounds.dispatch_game_sound(gs);
    }
    for (key, layer) in crate::matrix_game::interface::sound::drain() {
        state.sounds.play(&key, layer, Interrupt::Interrupt);
    }
    state
        .sounds
        .takt((crate::platform::now_secs() * 1000.0) as i64);
}

/// Parse the current page URL for a `?bundle=<url>` parameter. Returns None
/// when the parameter isn't set or the URL APIs aren't available.
#[cfg(target_arch = "wasm32")]
fn hide_loading_overlay() {
    if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
        if let Some(el) = doc.get_element_by_id("loading") {
            let _ = el.set_attribute("class", "fade");
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn bundle_url_from_query() -> Option<String> {
    let location = web_sys::window()?.location();
    let search = location.search().ok()?;
    if search.is_empty() {
        return None;
    }
    let params = web_sys::UrlSearchParams::new_with_str(&search).ok()?;
    params.get("bundle").filter(|s| !s.is_empty())
}

/// Construct the "Pause" hint widget — port of MatrixLogic.cpp:2611:
///
/// ```text
/// m_PauseHint = CMatrixHint::Build(TEMPLATE_PAUSE);
/// m_PauseHint->Show(14, 62);
/// ```
///
/// The hint goes through the standard hint render path so it picks up
/// the engine's chrome (border, background) and font choices baked
/// into the `Pause` template in `da/Templates`. Returns `None` when
/// the template is missing — older / stripped data files that don't
/// ship `Pause` just won't show a label, only the dim tint.
// ── Dialog mode — port of EnterDialogMode / LeaveDialogMode and the
// hint-button handlers (MatrixMap.cpp:3261-3574) ─────────────────────

/// Build one dialog hint widget from an assembled template. Returns
/// the hint with `screen_x/y` unset (caller positions it) plus the
/// copy positions (hint-local design px) for the button slots.
fn build_dialog_hint(
    state: &mut AppState,
    raw: &str,
) -> Option<(crate::matrix_game::interface::Hint, Vec<(i32, i32)>)> {
    use crate::matrix_game::interface::hint::{build_hint, center_text_parts, ChromeBorder};

    // First template field is the border id (MatrixHint.cpp:579) —
    // resolve it ourselves since `build_hint` only reports it back.
    let border_id: i32 = raw
        .split('|')
        .next()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    let chrome = &state.iface_list.hint_chrome;
    let border_default = ChromeBorder::default();
    let border = chrome.borders.get(&border_id).unwrap_or(&border_default);

    let mut sizes: std::collections::HashMap<String, (i32, i32)> = std::collections::HashMap::new();
    for bmp in chrome.bitmaps.values() {
        if let Some((w, h)) = state.iface_renderer.atlas_size(&bmp.path) {
            sizes.insert(bmp.path.clone(), (w as i32, h as i32));
        }
    }
    let sizer = |path: &str| sizes.get(path).copied();

    let replacer = state.iface_list.hint_replacer.clone();
    let atlas = state.iface_renderer.glyph_atlas_mut();
    let built = build_hint(raw, border, &chrome.bitmaps, &replacer, atlas, sizer)?;
    let crate::matrix_game::interface::hint::BuiltHint {
        mut parts,
        otstup,
        total_w,
        total_h,
        copy_pos,
        sound_in,
        sound_out,
        ..
    } = built;
    if parts.is_empty() {
        return None;
    }
    center_text_parts(&mut parts, state.iface_renderer.glyph_atlas_mut());
    // Both C++ call paths show the hint right after build —
    // Show()/SetVisible(true) fires SoundIn (MatrixHint.hpp:145-159,
    // the Menu fires it explicitly at MatrixMap.cpp:3491).
    if let Some(s) = &sound_in {
        crate::matrix_game::interface::sound::play_hint_sound(s);
    }
    Some((
        crate::matrix_game::interface::Hint {
            parts,
            total_w,
            total_h,
            border_id,
            otstup,
            screen_x: 0.0,
            screen_y: 0.0,
            sound_out,
        },
        copy_pos,
    ))
}

/// PLDropAllActions (MatrixSide.cpp:1684-1698): cancel turret
/// build + armed orders, close popup + constructor, deselect all.
fn pl_drop_all_actions(state: &mut AppState) {
    state.iface_list.turret_build.cancel();
    state.iface_list.pre_order = None;
    state.iface_list.popup = None;
    state.iface_list.popup_restore_pending = None;
    if let Some(b) = state.game.player_side.builder.as_mut() {
        b.deactivate();
    }
    state.game.select_nothing();
}

/// Port of `CMatrixMap::EnterDialogMode` (MatrixMap.cpp:3433-3574).
fn enter_dialog_mode(state: &mut AppState, name: &str) {
    use crate::matrix_game::interface::dialog::*;

    // The C++ releases m_PauseHint here; ours rebuilds per frame and
    // is gated on `dialog.is_none()`, so there's nothing to drop.
    leave_dialog_mode(state);
    // Pause(true) → CSound::StopPlayAllSounds (MatrixMap.hpp:634-639) —
    // kills looped voices (repair hum etc.) that logic won't re-pan
    // while frozen.
    state.is_paused = true;
    state.sounds.stop_all();

    if name != "Begin" {
        pl_drop_all_actions(state);
    }

    // StatD renders the same `Stat` template — only the OK handler
    // differs (MatrixMap.cpp:3460-3463, 3522-3530).
    let lookup = if name == "StatD" { "Stat" } else { name };
    let templates = state.iface_list.hint_templates.dialog_templates(lookup);
    if templates.is_empty() {
        log::warn!("dialog: no '{lookup}' template in da/Templates");
    }

    let screen_w = state.gfx.config.width as f32;
    let screen_h = state.gfx.config.height as f32;
    let scale = (screen_h / crate::matrix_game::interface::interface::DESIGN_H).max(0.1);

    let mut dialog = DialogState::new(name);
    let mut ww = 20.0 * scale;
    let hh = 62.0 * scale;
    for raw in &templates {
        let Some((mut hint, copy_pos)) = build_dialog_hint(state, raw) else {
            continue;
        };
        let w_px = hint.total_w as f32 * scale;
        let h_px = hint.total_h as f32 * scale;
        if lookup == "Menu" || lookup == "Stat" {
            // Centered with the 0.09*screen_h lift (MatrixMap.cpp:
            // 3486-3497). The C++ keeps the Menu invisible until a
            // camera fly-in ends (SetVisible(false) + the camera's
            // CAM_NEED2END_DIALOG_MODE dance) — that animation isn't
            // ported, so the Menu shows immediately.
            hint.screen_x = (screen_w - w_px) / 2.0;
            hint.screen_y = (screen_h - h_px) / 2.0 - (screen_h * 0.09).round();
        } else {
            hint.screen_x = ww;
            hint.screen_y = hh;
        }
        // Hint-local copy pos → Hints-panel design coords (the panel
        // origin resolves to (0,0): its design pos is 0/0).
        let btn = |i: usize| -> Option<(f32, f32)> {
            copy_pos.get(i).map(|&(x, y)| {
                (
                    hint.screen_x / scale + x as f32,
                    hint.screen_y / scale + y as f32,
                )
            })
        };
        // Button table per dialog (MatrixMap.cpp:3505-3565).
        let slot0: (&'static str, DialogAction) = match lookup {
            "Menu" => (HINT_CONTINUE, DialogAction::LeaveDialog),
            "Win" => (HINT_OK, DialogAction::OkWin),
            "Loose" => (HINT_OK, DialogAction::OkLoose),
            "Stat" if name == "StatD" => (HINT_OK, DialogAction::OkStatBack),
            "Stat" => (HINT_OK, DialogAction::OkStatExit),
            _ => (HINT_OK, DialogAction::LeaveDialog),
        };
        let slots: [(&'static str, DialogAction); 6] = [
            slot0,
            (HINT_RESET, DialogAction::ResetRequest),
            // HelpRequestHandler (MatrixMap.cpp:3404-3426) leaves the
            // dialog then spawns Manual.exe — there is no manual to
            // launch in the port, so only the leave remains.
            (HINT_HELP, DialogAction::LeaveDialog),
            (HINT_SURRENDER, DialogAction::SurrenderRequest),
            (HINT_EXIT, DialogAction::ExitRequest),
            (HINT_CANCEL_MENU, DialogAction::LeaveDialog),
        ];
        for (i, (elem, action)) in slots.into_iter().enumerate() {
            if let Some(pos) = btn(i) {
                dialog.buttons.push(DialogButton {
                    elem,
                    action,
                    pos,
                    disabled: false,
                });
            }
        }
        ww += w_px + 20.0 * scale;
        dialog.hints.push(hint);
    }
    log::info!(
        "dialog: entered '{}' ({} hint(s), {} button(s))",
        name,
        dialog.hints.len(),
        dialog.buttons.len()
    );
    state.dialog = Some(dialog);
    sync_dialog_buttons(state);
}

/// Port of `CMatrixMap::LeaveDialogMode` (MatrixMap.cpp:3261-3283).
fn leave_dialog_mode(state: &mut AppState) {
    let Some(dialog) = state.dialog.take() else {
        return;
    };
    // The Menu fires its hint's SoundOut on leave (MatrixMap.cpp:3267);
    // other dialogs just Release their hints, which is silent.
    if dialog.is_menu() {
        if let Some(s) = dialog.hints.first().and_then(|h| h.sound_out.as_ref()) {
            crate::matrix_game::interface::sound::play_hint_sound(s);
        }
    }
    state.is_paused = false; // Pause(false)
    sync_dialog_buttons(state); // HideHintButtons
}

/// Push the dialog's button set onto the `if/Hints` panel — the
/// CreateHintButton / HideHintButtons / Disable-EnableMainMenuButton
/// effects (CInterface.cpp:4208-4310) in one reconcile.
fn sync_dialog_buttons(state: &mut AppState) {
    use crate::matrix_game::interface::ElementState;
    let buttons: Vec<(&'static str, (f32, f32), bool)> = state
        .dialog
        .as_ref()
        .map(|d| {
            d.buttons
                .iter()
                .map(|b| (b.elem, b.pos, b.disabled))
                .collect()
        })
        .unwrap_or_default();
    let active = state.dialog.is_some();
    let Some(p) = state.iface_list.panel_mut("Hints") else {
        return;
    };
    p.visible = active;
    for e in p.elements.iter_mut() {
        e.set_visible(false);
    }
    for (name, pos, disabled) in buttons {
        if let Some(e) = p.elements.iter_mut().find(|e| e.name == name) {
            e.pos_x = pos.0;
            e.pos_y = pos.1;
            e.set_visible(true);
            e.cur_state = if disabled {
                ElementState::Disabled
            } else {
                ElementState::Normal
            };
            e.def_state = e.cur_state;
        }
    }
}

/// Port of `CreateConfirmation` (MatrixMap.cpp:3359-3392).
fn create_confirmation(
    state: &mut AppState,
    template: &str,
    ok_action: crate::matrix_game::interface::dialog::DialogAction,
) {
    let Some(raw) = state
        .iface_list
        .hint_templates
        .get(template)
        .map(str::to_string)
    else {
        return;
    };
    let Some((mut hint, copy_pos)) = build_dialog_hint(state, &raw) else {
        return;
    };
    let screen_w = state.gfx.config.width as f32;
    let screen_h = state.gfx.config.height as f32;
    let scale = (screen_h / crate::matrix_game::interface::interface::DESIGN_H).max(0.1);
    hint.screen_x = (screen_w - hint.total_w as f32 * scale) / 2.0;
    hint.screen_y = (screen_h - hint.total_h as f32 * scale) / 2.0 - (screen_h * 0.09).round();
    let design: Vec<(f32, f32)> = copy_pos
        .iter()
        .map(|&(x, y)| {
            (
                hint.screen_x / scale + x as f32,
                hint.screen_y / scale + y as f32,
            )
        })
        .collect();
    if let Some(d) = state.dialog.as_mut() {
        d.open_confirmation(hint, &design, ok_action);
    }
    sync_dialog_buttons(state);
}

/// MMFLAG_STAT_DIALOG(/_D) consumption — port of MatrixLogic.cpp:
/// 2527-2596: build the per-side stat replacements into the Replace
/// set, then open the Stat (exit path) or StatD (back-to-game) dialog.
fn enter_stat_dialog(state: &mut AppState, back_to_game: bool) {
    use crate::matrix_game::interface::dialog::{build_stat_replacements, SideStatRow};
    let mut rows = vec![SideStatRow {
        prefix: "_p_".to_string(),
        stats: state.game.player_side.statistic,
    }];
    for s in &state.game.other_sides {
        let Some(prefix) = state.iface_list.side_prefixes.get(&s.id) else {
            continue;
        };
        rows.push(SideStatRow {
            prefix: prefix.clone(),
            stats: s.statistic,
        });
    }
    for (k, v) in build_stat_replacements(&rows) {
        state.iface_list.hint_replacer.set(k, v);
    }
    enter_dialog_mode(state, if back_to_game { "StatD" } else { "Stat" });
}

/// `PressHintButton` (CInterface.cpp:4311-4323) — fire the stored
/// handler of a named hint button (skips disabled buttons).
fn press_hint_button(state: &mut AppState, elem: &str) {
    let Some(action) = state.dialog.as_ref().and_then(|d| d.action_of(elem)) else {
        return;
    };
    dispatch_dialog_action(state, action);
}

/// The dialog-button handler table (MatrixMap.cpp:3285-3431).
fn dispatch_dialog_action(
    state: &mut AppState,
    action: crate::matrix_game::interface::dialog::DialogAction,
) {
    use crate::matrix_game::interface::dialog::DialogAction as A;
    use crate::matrix_game::side::{SideStatus, Stat};
    match action {
        A::LeaveDialog => leave_dialog_mode(state),
        A::ExitRequest => create_confirmation(state, "Exit", A::ConfirmExit),
        A::ResetRequest => create_confirmation(state, "Reset", A::ConfirmReset),
        A::SurrenderRequest => create_confirmation(state, "Surrender", A::ConfirmSurrender),
        A::ConfirmCancel => {
            if let Some(d) = state.dialog.as_mut() {
                d.cancel_confirmation();
            }
            sync_dialog_buttons(state);
        }
        // The C++ Ok*Handlers set g_ExitState + MMFLAG_STAT_DIALOG and
        // let the next logic takt build the stat dialog; we do it
        // synchronously — same observable result without the flag hop.
        A::ConfirmExit => {
            leave_dialog_mode(state);
            state.exit_state = 1;
            enter_stat_dialog(state, false);
        }
        A::ConfirmReset => {
            leave_dialog_mode(state);
            restart_game();
        }
        A::ConfirmSurrender => {
            leave_dialog_mode(state);
            state.exit_state = 4;
            // Negate every active non-player side's STAT_TIME so the
            // stat dialog shows them green (MatrixMap.cpp:3322-3329).
            for s in state.game.other_sides.iter_mut() {
                if s.status == SideStatus::Active {
                    let t = s.get_stat(Stat::Time);
                    s.set_stat(Stat::Time, -t);
                }
            }
            enter_stat_dialog(state, false);
        }
        A::OkWin => {
            leave_dialog_mode(state);
            state.exit_state = 3;
            enter_stat_dialog(state, false);
        }
        A::OkLoose => {
            leave_dialog_mode(state);
            state.exit_state = 2;
            enter_stat_dialog(state, false);
        }
        A::OkStatExit => {
            leave_dialog_mode(state);
            #[cfg(not(target_arch = "wasm32"))]
            write_host_result(state.exit_state, &state.game);
            exit_game(state.exit_state);
        }
        A::OkStatBack => leave_dialog_mode(state),
    }
}

/// `OkJustExitHandler` sets GFLAG_EXITLOOP and the host (SR2 / the
/// standalone EXE shell) tears the session down. The port's "host" on
/// the web is the map-select menu (index.html without `?bundle=`), so
/// exiting navigates back there; natively we exit the process.
fn exit_game(exit_state: i32) {
    log::info!("dialog: leaving game loop (g_ExitState={exit_state})");
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(w) = web_sys::window() {
            let loc = w.location();
            match loc.pathname() {
                Ok(path) => {
                    let _ = loc.set_href(&path);
                }
                Err(_) => {
                    let _ = loc.reload();
                }
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    std::process::exit(0);
}

/// `OkResetHandler` → `g_MatrixMap->Restart()` reloads the whole map
/// in place. Recreating the session is equivalent: page reload on the
/// web; on native we exit (simplest faithful option — an in-process
/// restart would mean rebuilding the entire AppState).
fn restart_game() {
    log::info!("dialog: restart requested");
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(w) = web_sys::window() {
            let _ = w.location().reload();
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Under the Rangers host the session must survive a restart, so
        // re-exec ourselves with the same arguments and environment.
        #[cfg(unix)]
        if std::env::var_os("MG_RESULT").is_some() {
            use std::os::unix::process::CommandExt;
            if let Ok(exe) = std::env::current_exe() {
                let err = std::process::Command::new(exe)
                    .args(std::env::args_os().skip(1))
                    .exec();
                log::error!("restart: exec failed: {err}");
            }
        }
        std::process::exit(0);
    }
}

// ── In-scope keyboard map (MatrixFormGame.cpp:1172-1560, non-cheat,
// non-arcade subset; default bindings MatrixConfig.cpp:252-319) ──────

/// Count of live group members carrying `kind` — `GetBombersCnt` /
/// `GetRepairsCnt` (MatrixAIGroup.cpp:387-424).
fn group_weapon_count(state: &AppState, kind: crate::matrix_game::config::RobotUnitKind) -> i32 {
    use crate::matrix_game::logic::robot_ref;
    use crate::matrix_game::map_static::MapStatic;
    let Some(g) = state.game.player_side.cur_group() else {
        return 0;
    };
    g.objects
        .iter()
        .filter(|m| {
            robot_ref(&state.game.objects, m.object)
                .map(|r| r.is_live() && r.config.weapon.iter().any(|w| w.kind == kind))
                .unwrap_or(false)
        })
        .count() as i32
}

/// KA_ORDER_ROBOT_SWITCH1/2 (`,` / `.`) — cycle the selection through
/// the player's live robots (MatrixFormGame.cpp:1532-1560: walk the
/// logic list prev/next from the single selected robot, wrap once).
fn robot_switch(state: &mut AppState, forward: bool) {
    use crate::matrix_game::logic::{get_world_pos, robot_ref};
    use crate::matrix_game::map_static::MapStatic;
    use crate::matrix_game::side::CurrSel;

    let side_id = state.game.player_side.id;
    let robots: Vec<crate::matrix_game::map_static::ObjectId> = state
        .game
        .objects
        .iter_units()
        .filter(|&id| {
            robot_ref(&state.game.objects, id)
                .map(|r| r.is_live() && r.side == side_id)
                .unwrap_or(false)
        })
        .collect();
    if robots.is_empty() {
        return;
    }
    // Anchor: the single selected live robot, if any (the C++ gate at
    // :1535 — cur group with exactly one live-robot member).
    let anchor = state
        .game
        .player_side
        .cur_group()
        .filter(|g| g.objects_cnt() == 1)
        .and_then(|g| g.objects.first().map(|m| m.object))
        .and_then(|id| robots.iter().position(|&r| r == id));
    let n = robots.len();
    let next = match anchor {
        Some(i) => {
            if forward {
                robots[(i + 1) % n]
            } else {
                robots[(i + n - 1) % n]
            }
        }
        // No anchor: '.' walks forward from the list head → first;
        // ',' walks backward, wraps once → last.
        None => {
            if forward {
                robots[0]
            } else {
                robots[n - 1]
            }
        }
    };
    // CreateGroupFromCurrent + Select(ROBOT) + camera recenter.
    state.game.player_side.selected = vec![next];
    state.game.player_side.active_object = Some(next);
    state.game.player_side.curr_sel = CurrSel::RobotsSelected;
    state.game.sync_group_from_selection();
    if let Some(p) = get_world_pos(&state.game.objects, next) {
        state.camera.set_xy_strategy([p.x, p.y]);
    }
}

/// Per-frame arcade-mode upkeep (the arcade slices of
/// `CMatrixMap::Takt` + `CMatrixGameForm::Takt`):
///   - `m_TraceStopPos` — camera-ray cursor trace skipping the
///     arcaded robot (MatrixMap.cpp:1149-1160);
///   - the `GetAsyncKeyState(KA_UNIT_*)` / `KA_FIRE` input snapshot
///     consumed by the robot logic takt;
///   - the CAMERA_INROBOT follow target (CalcLinkPoint inputs,
///     MatrixCamera.cpp:899-915);
///   - the Main-panel ±196px arcade slide (CInterface.cpp:3371/3392),
///     reconciled so a robot death restores the panel too.
fn sync_arcade_mode(state: &mut AppState) {
    use crate::matrix_game::camera::ArcadeCamTarget;
    use crate::matrix_game::map_static::MapStatic;
    use winit::keyboard::KeyCode;

    let arcaded = state.game.objects.arcaded_object.filter(|&id| {
        crate::matrix_game::logic::robot_ref(&state.game.objects, id)
            .map(|r| r.is_live())
            .unwrap_or(false)
    });

    if let Some(aid) = arcaded {
        let held = |c: KeyCode| state.held_keys.contains(&c);
        let w = state.gfx.config.width as f32;
        let h = state.gfx.config.height as f32;
        let [mx, my] = state.cursor;
        let over_ui = mx >= 0.0 && state.iface_list.hit_test(mx, my, w, h).is_some();
        let mut cursor_world = state.game.objects.arcade_input.cursor_world;
        if mx >= 0.0 && !state.is_paused {
            use crate::matrix_game::common::TRACE_ALL;
            use crate::matrix_game::map_trace::trace;
            let (o, d) = state.camera.screen_to_world_ray(mx, my, w, h);
            let (_stop, pos) = trace(
                &state.map,
                &state.game.objects,
                o,
                o + d * 10_000.0,
                TRACE_ALL,
                Some(aid),
            );
            cursor_world = pos;
        }
        state.game.objects.arcade_input = crate::matrix_game::map_static::ArcadeInput {
            left: held(KeyCode::KeyA) || held(KeyCode::ArrowLeft),
            right: held(KeyCode::KeyD) || held(KeyCode::ArrowRight),
            forward: held(KeyCode::KeyW) || held(KeyCode::ArrowUp),
            backward: held(KeyCode::KeyS) || held(KeyCode::ArrowDown),
            fire: state.lmb_down,
            cursor_world,
            cursor_on_interface: over_ui,
        };
        if let Some(r) = crate::matrix_game::logic::robot_ref(&state.game.objects, aid) {
            let speed_ratio = (r.speed / r.max_speed().max(1e-6)).clamp(0.0, 1.0);
            state.camera.set_arcade_target(Some(ArcadeCamTarget {
                pos: glam::Vec3::new(r.pos_x, r.pos_y, r.pos_z),
                forward: r.forward,
                speed_ratio,
            }));
        }
    } else {
        state.camera.set_arcade_target(None);
    }

    // Main-panel slide reconciler (relative shift keeps the data-
    // driven base xPos authoritative).
    let want = arcaded.is_some();
    if want != state.main_panel_arcade {
        if let Some(p) = state.iface_list.panel_mut("Main") {
            p.design_x += if want { 196.0 } else { -196.0 };
        }
        state.main_panel_arcade = want;
    }
}

/// `CIFaceList::EnterRobot` (CInterface.cpp:3377-3399) — hand the
/// currently-selected robot over to manual control.
fn enter_robot(state: &mut AppState) {
    let Some(id) = state.game.player_side.get_cur_sel_object() else {
        return;
    };
    state.game.set_arcaded_object(&state.map, Some(id));
}

/// `CIFaceList::LiveRobot` (CInterface.cpp:3340-3375) — leave arcade
/// mode and re-select the robot in strategy mode.
fn leave_robot(state: &mut AppState) {
    use crate::matrix_game::map_static::MapStatic;
    use crate::matrix_game::side::CurrSel;
    if !state.game.is_arcade_mode() {
        return;
    }
    let obj = state.game.objects.arcaded_object;
    state.game.set_arcaded_object(&state.map, None);
    if let Some(id) = obj {
        if crate::matrix_game::logic::robot_ref(&state.game.objects, id)
            .map(|r| r.is_live())
            .unwrap_or(false)
        {
            state.game.player_side.selected = vec![id];
            state.game.player_side.active_object = Some(id);
            state.game.player_side.curr_sel = CurrSel::RobotsSelected;
            state.game.sync_group_from_selection();
        }
    }
}

/// Constructor quick-pick: digit row → (type, kind, pylon) per the
/// focused pylon (MatrixFormGame.cpp:1421-1493). `key` is the typed
/// digit (1..9, 0).
fn constructor_quick_pick(state: &mut AppState, key: i32) -> bool {
    use crate::matrix_game::config::RobotUnitKind;
    use crate::matrix_game::object_robot::RobotUnitType;

    let focused = state
        .game
        .player_side
        .builder
        .as_ref()
        .filter(|b| b.active)
        .and_then(|b| b.focused_element.clone());
    let Some(focused) = focused else {
        return false;
    };

    let mut ty = RobotUnitType::Empty;
    let mut kind = 0i32; // RUK_UNKNOWN
    let mut pilon = -1i32;
    match focused.as_str() {
        "pi1" | "pi2" | "pi3" | "pi4" => {
            pilon = focused.as_bytes()[2] as i32 - b'1' as i32;
            ty = RobotUnitType::Weapon;
        }
        "pi5" if key <= 2 => {
            pilon = 4;
            ty = RobotUnitType::Weapon;
            kind = match key {
                1 => RobotUnitKind::WEAPON_MORTAR.0,
                2 => RobotUnitKind::WEAPON_BOMB.0,
                _ => key,
            };
        }
        "pihe" if key <= 4 => {
            pilon = 0;
            ty = RobotUnitType::Head;
            kind = key;
        }
        "pihu" if key != 0 && key <= 6 => {
            pilon = 0;
            ty = RobotUnitType::Armor;
            kind = if key == 1 { 6 } else { key - 1 };
        }
        "pich" if key != 0 && key <= 5 => {
            pilon = 0;
            ty = RobotUnitType::Chassis;
            kind = key;
        }
        _ => {}
    }
    // Common-pylon weapon row skips mortar(5)/bomb(7): 5→laser(6),
    // 6..8 → +2 (plasma/electric/repair). MatrixFormGame.cpp:1481-1491.
    if ty == RobotUnitType::Weapon && pilon < 4 && key <= 8 {
        kind = match key {
            0 => 0,
            5 => 6,
            k if k > 5 => k + 2,
            k => k,
        };
    }
    if ty == RobotUnitType::Empty {
        // A digit on a focused pylon is consumed even when it maps to
        // nothing (the C++ falls through to SuperDjeans only on a
        // valid type but never reaches other handlers).
        return true;
    }
    if let Some(b) = state.game.player_side.builder.as_mut() {
        b.super_djeans(ty, RobotUnitKind(kind), pilon, false);
    }
    reset_build_counter(state);
    true
}

/// Strategy-mode keyboard dispatch. Returns true when the key was
/// consumed (the camera bindings must not see it).
fn handle_game_key(state: &mut AppState, code: winit::keyboard::KeyCode) -> bool {
    use crate::matrix_game::config::RobotUnitKind;
    use crate::matrix_game::interface::dialog::{DialogAction, HINT_OK};
    use crate::matrix_game::interface::Click;
    use crate::matrix_game::side::CurrSel;
    use winit::keyboard::KeyCode as K;

    // ── Dialog-mode keys (MatrixFormGame.cpp:1172-1264) ──
    if let Some(d) = state.dialog.as_ref() {
        let is_menu = d.is_menu();
        let has_confirmation = d.has_confirmation();
        match code {
            K::Enter | K::NumpadEnter if d.enter_presses_ok() => {
                press_hint_button(state, HINT_OK);
            }
            K::KeyE if is_menu && !has_confirmation => {
                dispatch_dialog_action(state, DialogAction::ExitRequest);
            }
            K::KeyS if is_menu && !has_confirmation => {
                dispatch_dialog_action(state, DialogAction::SurrenderRequest);
            }
            K::KeyR if is_menu && !has_confirmation => {
                dispatch_dialog_action(state, DialogAction::ResetRequest);
            }
            K::Escape if is_menu => {
                if has_confirmation {
                    dispatch_dialog_action(state, DialogAction::ConfirmCancel);
                } else {
                    leave_dialog_mode(state);
                }
            }
            _ => {}
        }
        // Everything else dies here. The C++ technically lets the
        // game keys fall through (only ESC checks MMFLAG_DIALOG_MODE,
        // :1239), but with the game paused and the selection dropped
        // they are no-ops at best — swallow them.
        return true;
    }

    match code {
        // ESC — leave the robot when arcaded (MatrixFormGame.cpp:
        // 1241-1245), else the main-menu dialog (:1263).
        K::Escape => {
            if state.game.is_arcade_mode() {
                leave_robot(state);
            } else {
                enter_dialog_mode(state, "Menu");
            }
            return true;
        }
        // KEY_PAUSE toggle (MatrixFormGame.cpp:1290-1292). Pause() is
        // a no-op in dialog mode (MatrixMap.hpp:636) — unreachable
        // here since the dialog branch above consumed the key.
        K::Pause => {
            state.is_paused = !state.is_paused;
            // Pause(true) stops all sounds (MatrixMap.hpp:634-639).
            if state.is_paused {
                state.sounds.stop_all();
            }
            return true;
        }
        _ => {}
    }

    // ── Robot (arcade) mode keys (MatrixFormGame.cpp:1294-1307) ──
    if state.game.is_arcade_mode() {
        match code {
            // KA_UNIT_BOOM ('E') — self-destruct.
            K::KeyE => {
                if let Some(id) = state.game.objects.arcaded_object {
                    state.game.robot_big_boom(&state.map, id);
                }
                return true;
            }
            // KA_UNIT_ENTER / _ALT (Enter / Space) — leave the robot.
            K::Enter | K::NumpadEnter | K::Space => {
                leave_robot(state);
                return true;
            }
            // Every strategy-mode game key below is inside the C++
            // `else` of `IsRobotMode()` (MatrixFormGame.cpp:1308) —
            // unreachable while arcaded. Camera pitch/zoom bindings
            // still apply (not consumed).
            _ => return false,
        }
    }

    let ordering =
        state.iface_list.pre_order.is_some() || state.iface_list.turret_build.is_active();
    let curr_sel = state.game.player_side.curr_sel;

    if !ordering {
        // ── Group selected (MatrixFormGame.cpp:1314-1380) ──
        if curr_sel == CurrSel::RobotsSelected && state.game.player_side.cur_group().is_some() {
            let (auto_cap, auto_atk, auto_def) = state.game.show_order_state();
            match code {
                // KA_UNIT_ENTER / _ALT (Enter / Space) — take manual
                // control of the active selected robot
                // (MatrixFormGame.cpp:1314-1321).
                K::Enter | K::NumpadEnter | K::Space => {
                    enter_robot(state);
                    return true;
                }
                // a"U"to attack toggle.
                K::KeyU => {
                    let no = state.game.sel_group_to_logic_group();
                    if auto_atk {
                        state.game.pg_order_stop(&state.map, no);
                    } else {
                        state.game.pg_order_auto_attack(&state.map, no);
                    }
                    return true;
                }
                // "C"apture toggle.
                K::KeyC => {
                    let no = state.game.sel_group_to_logic_group();
                    if auto_cap {
                        state.game.pg_order_stop(&state.map, no);
                    } else {
                        state.game.pg_order_auto_capture(&state.map, no);
                    }
                    return true;
                }
                // "D"efend toggle.
                K::KeyD => {
                    let no = state.game.sel_group_to_logic_group();
                    if auto_def {
                        state.game.pg_order_stop(&state.map, no);
                    } else {
                        state.game.pg_order_auto_defence(&state.map, no);
                    }
                    return true;
                }
                // "M"ove pre-order.
                K::KeyM => {
                    dispatch_ui_click(state, &Click::Button("ogo".into()));
                    return true;
                }
                // "S"top (+ break orders, MatrixFormGame.cpp:1356-1359).
                K::KeyS => {
                    state.iface_list.pre_order = None;
                    state.game.order_stop(&state.map);
                    state.game.selected_group_break_orders();
                    return true;
                }
                // Ta"K"e (capture) pre-order.
                K::KeyK => {
                    dispatch_ui_click(state, &Click::Button("oca".into()));
                    return true;
                }
                // "P"atrol pre-order.
                K::KeyP => {
                    dispatch_ui_click(state, &Click::Button("opa".into()));
                    return true;
                }
                // "E"xplode — only when the group has bombers.
                K::KeyE if group_weapon_count(state, RobotUnitKind::WEAPON_BOMB) > 0 => {
                    dispatch_ui_click(state, &Click::Button("obomb".into()));
                    return true;
                }
                // "R"epair — only when the group has repairers.
                K::KeyR if group_weapon_count(state, RobotUnitKind::WEAPON_REPAIR) > 0 => {
                    dispatch_ui_click(state, &Click::Button("orep".into()));
                    return true;
                }
                // "A"ttack pre-order.
                K::KeyA => {
                    dispatch_ui_click(state, &Click::Button("ofi".into()));
                    return true;
                }
                _ => {}
            }
        } else if matches!(curr_sel, CurrSel::BaseSelected | CurrSel::BuildingSelected) {
            // ── Building / base selected (MatrixFormGame.cpp:1381-1493) ──
            let builder_active = state
                .game
                .player_side
                .builder
                .as_ref()
                .map(|b| b.active)
                .unwrap_or(false);
            match code {
                // "B"ase — open the robot constructor.
                K::KeyB if curr_sel == CurrSel::BaseSelected && !builder_active => {
                    dispatch_ui_click(state, &Click::Button("buro".into()));
                    return true;
                }
                // "T"urret picker — same gate as the `buca` button
                // (free slot + affordable + stack not full); the
                // per-frame visibility refresh keeps the element state
                // current, so read the gate off it.
                K::KeyT => {
                    let buca_enabled = state
                        .iface_list
                        .panel("Main")
                        .and_then(|p| p.elements.iter().find(|e| e.name == "buca"))
                        .map(|e| {
                            !matches!(
                                e.cur_state,
                                crate::matrix_game::interface::ElementState::Disabled
                            )
                        })
                        .unwrap_or(false);
                    if buca_enabled {
                        // The C++ also closes the constructor panel
                        // (ResetGroupNClose, :1403) before flipping
                        // PREORDER_BUILD_TURRET.
                        if builder_active {
                            if let Some(b) = state.game.player_side.builder.as_mut() {
                                b.deactivate();
                            }
                            state.is_paused = false;
                        }
                        dispatch_ui_click(state, &Click::Button("buca".into()));
                    }
                    return true;
                }
                // Digit quick-pick on the focused constructor pylon.
                K::Digit1
                | K::Digit2
                | K::Digit3
                | K::Digit4
                | K::Digit5
                | K::Digit6
                | K::Digit7
                | K::Digit8
                | K::Digit9
                | K::Digit0 => {
                    let key = match code {
                        K::Digit0 => 0,
                        K::Digit1 => 1,
                        K::Digit2 => 2,
                        K::Digit3 => 3,
                        K::Digit4 => 4,
                        K::Digit5 => 5,
                        K::Digit6 => 6,
                        K::Digit7 => 7,
                        K::Digit8 => 8,
                        _ => 9,
                    };
                    if constructor_quick_pick(state, key) {
                        return true;
                    }
                }
                _ => {}
            }
        }
    } else {
        // ── Ordering / turret-build mode (MatrixFormGame.cpp:1495-1524) ──
        let picker_open = state.iface_list.turret_build.is_active()
            && state.iface_list.turret_build.kind.is_none();
        if picker_open {
            // KA_TURRET_CANNON/GUN/LASER/ROCKET — C/G/L/R in the
            // default bindings (MatrixConfig.cpp:316-319); the digit
            // row is accepted too as a convenience alias.
            let kind_n = match code {
                K::KeyC | K::Digit1 => 1,
                K::KeyG | K::Digit2 => 2,
                K::KeyL | K::Digit3 => 3,
                K::KeyR | K::Digit4 => 4,
                _ => 0,
            };
            if kind_n > 0 {
                dispatch_ui_click(state, &Click::Button(format!("tur{kind_n}")));
                return true;
            }
        }
        // KA_ORDER_CANCEL ("X") — cancel turret build / ordering mode.
        if code == K::KeyX {
            dispatch_ui_click(state, &Click::Button("ocan".into()));
            return true;
        }
    }

    // ── Common strategy keys (MatrixFormGame.cpp:1526-1560) ──
    match code {
        // KA_MINIMAP_ZOOMIN/OUT (VK_OEM_PLUS / VK_OEM_MINUS).
        K::Equal | K::NumpadAdd => {
            state.minimap.zoom_in();
            true
        }
        K::Minus | K::NumpadSubtract => {
            state.minimap.zoom_out();
            true
        }
        // KA_ORDER_ROBOT_SWITCH1/2 ("," / ".").
        K::Comma => {
            robot_switch(state, false);
            true
        }
        K::Period => {
            robot_switch(state, true);
            true
        }
        _ => false,
    }
}

fn build_pause_hint(state: &mut AppState) -> Option<crate::matrix_game::interface::Hint> {
    use crate::matrix_game::interface::hint::{build_hint, ChromeBorder};

    let raw = state.iface_list.hint_templates.get("Pause")?;
    let raw = raw.to_string();
    let chrome = &state.iface_list.hint_chrome;
    let border_default = ChromeBorder::default();
    let border = chrome.borders.get(&0).unwrap_or(&border_default);

    let screen_w = state.gfx.config.width as f32;
    let screen_h = state.gfx.config.height as f32;
    let scale = (screen_h / crate::matrix_game::interface::interface::DESIGN_H).max(0.1);

    // Snapshot bitmap sizes for the hint layout — same pattern as the
    // hover-hint update path (see the `iface_list.update` call
    // elsewhere in this file).
    let mut sizes: std::collections::HashMap<String, (i32, i32)> = std::collections::HashMap::new();
    for bmp in chrome.bitmaps.values() {
        if let Some((w, h)) = state.iface_renderer.atlas_size(&bmp.path) {
            sizes.insert(bmp.path.clone(), (w as i32, h as i32));
        }
    }
    let sizer = |path: &str| sizes.get(path).copied();

    let replacer = state.iface_list.hint_replacer.clone();
    let atlas = state.iface_renderer.glyph_atlas_mut();
    let built = build_hint(&raw, border, &chrome.bitmaps, &replacer, atlas, sizer)?;
    let crate::matrix_game::interface::hint::BuiltHint {
        mut parts,
        otstup,
        total_w,
        total_h,
        border_id,
        ..
    } = built;
    if parts.is_empty() {
        return None;
    }

    // Center text horizontally within the hint's content area. The
    // original C++ Rangers rasterizer centers text inside its bounding
    // box by default (`alignx = 1` at MatrixHint.cpp:590); our
    // hint renderer renders `HintPart::Text` left-aligned at `x`, and
    // when the template uses `_WIDTH:N` the part's `w` may be wider
    // than the actual glyph run. Re-measure and shift each text part
    // so its glyph run sits centered within the hint's content area.
    let content_w = total_w - otstup[0] - otstup[2];
    let atlas = state.iface_renderer.glyph_atlas_mut();
    for part in parts.iter_mut() {
        if let crate::matrix_game::interface::hint::HintPart::Text { x, text, font, .. } = part {
            // Use the longest line for measurement when text is
            // multi-line (e.g. `<br>` separator). That preserves the
            // visual block centering the C++ produces.
            let measured: i32 = text
                .split('\n')
                .map(|line| atlas.measure(font, line) as i32)
                .max()
                .unwrap_or(0);
            *x = otstup[0] + (content_w - measured) / 2;
        }
    }

    // C++ position from MatrixLogic.cpp:2612 — `Show(14, 62)` is in
    // design-space pixels (1024×768). Scale to current screen.
    let screen_x = 14.0 * scale;
    let screen_y = 62.0 * scale;
    // Clamp so it doesn't fall off-screen on tiny windows.
    let screen_x = screen_x
        .min(screen_w - total_w as f32 * scale - 2.0)
        .max(0.0);
    let screen_y = screen_y
        .min(screen_h - total_h as f32 * scale - 2.0)
        .max(0.0);

    Some(crate::matrix_game::interface::Hint {
        parts,
        total_w,
        total_h,
        border_id,
        otstup,
        screen_x,
        screen_y,
        sound_out: None,
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn run() {
    let event_loop = EventLoop::new().expect("failed to create event loop");
    let mut app = App {
        state: Rc::new(RefCell::new(None)),
    };
    event_loop.run_app(&mut app).expect("event loop error");
}

#[cfg(target_arch = "wasm32")]
pub fn run() {
    use winit::platform::web::EventLoopExtWebSys;
    crate::matrix_game::keybinds::init();
    let event_loop = EventLoop::new().expect("failed to create event loop");
    let app = App {
        state: Rc::new(RefCell::new(None)),
    };
    event_loop.spawn_app(app);
}

#[cfg(test)]
mod tests {
    use super::merge_record;

    #[test]
    fn merge_record_updates_only_on_improvement() {
        assert_eq!(merge_record("", "atoll", 5000), Some("atoll=5000".into()));
        assert_eq!(
            merge_record("atoll=5000", "training", 900),
            Some("atoll=5000;training=900".into())
        );
        assert_eq!(
            merge_record("atoll=5000;training=900", "atoll", 4000),
            Some("atoll=4000;training=900".into())
        );
        assert_eq!(merge_record("atoll=4000;training=900", "atoll", 4000), None);
        assert_eq!(merge_record("atoll=4000;training=900", "atoll", 9999), None);
        // garbage entries are dropped rather than crashing
        assert_eq!(merge_record("bogus;=;x=y", "atoll", 10), Some("atoll=10".into()));
    }
}
