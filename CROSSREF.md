# Rust ↔ Original C++ File Cross-Reference

Mapping between Rust port modules and original Space Rangers 2 files.
When adding a new Rust file, place it where this table predicts — if
there is no row yet, extend the table in the same PR.

Layout mirrors the original tree:

    MatrixLib/Base/      <-> matrix_lib/base/
    MatrixLib/Bitmap/    <-> matrix_lib/bitmap/
    MatrixLib/3G/        <-> matrix_lib/three_g/
    MatrixGame/src/      <-> matrix_game/
    MatrixGame/src/Effects/    <-> matrix_game/effects/
    MatrixGame/src/Interface/  <-> matrix_game/interface/
    MatrixGame/src/Logic/      <-> matrix_game/logic/

`gfx/` and `platform/` have no C++ counterpart — they hold wgpu and
WASM/native glue that replaces the original Windows/DirectX platform
layer.

## matrix_lib/base/ (MatrixLib/Base/)

| Rust file                         | Original                        |
|-----------------------------------|---------------------------------|
| matrix_lib/base/blockpar.rs       | MatrixLib/Base/src/CBlockPar.cpp|
| matrix_lib/base/pack.rs           | MatrixLib/Base/src/Pack.cpp     |
| matrix_lib/base/storage.rs        | MatrixLib/Base/src/CStorage.cpp |
| matrix_lib/base/wstr.rs           | MatrixLib/Base/src/CWStr.cpp (field parsers only: `GetStrPar`, `GetIntPar`, `GetDoublePar`, `GetCountPar`, `CompareFirst`) |

N/A-by-design (C++/Win32 infrastructure replaced by Rust std):
CBuf/CFile (buffered file IO → `Vec<u8>` + gfx/loader.rs), CDWORDMap/
CList (containers → std collections), CException/Tracer (SEH + DTRACE
call-stack tracer → Result/panic), CHeap/Mem (allocator bookkeeping →
the Rust allocator), CMain (static-init base class), CStr/CWStrFormat
(string classes → `String`; CWStr's read-only field parsers are the
part with real behavior and live in `wstr.rs`), Registry (Win32
registry persistence — no game-code callers), CReminder (delayed
D3D-resource-eviction timers; wgpu resources are kept for the session),
CRC32 (only callers are `CStorage::Save`/`CalcUniqID` and
`CBitmap::Hash`, none reachable from the game).

## matrix_lib/bitmap/ (MatrixLib/Bitmap/)

| Rust file                         | Original                        |
|-----------------------------------|---------------------------------|
| matrix_lib/bitmap/mod.rs          | MatrixLib/Bitmap/src/CBitmap.cpp|
| matrix_lib/bitmap/sharpen.rs      | MatrixLib/Bitmap/src/sharpen.cpp + asharpen.asm (CPU reference; shaders/icon_sharpen.wgsl mirrors it for the robot-icon mip chain) |

## matrix_lib/three_g/ (MatrixLib/3G/)

| Rust file                               | Original                           |
|-----------------------------------------|------------------------------------|
| matrix_lib/three_g/billboard.rs         | MatrixLib/3G/src/CBillboard.cpp (queue + quad/line expansion; GPU flush in effects_renderer.rs) |
| matrix_lib/three_g/math3d.rs            | MatrixLib/3G/src/Math3D.cpp (CTrajectory + VecToMatrixX/Y; the other live helpers — IntersectTriangle, CalcPick, IsIntersectSphere, BuildRotateMatrix, Vec3Projection — are inlined at their Rust call sites: map_trace.rs, camera.rs, side_player.rs, lightening.rs, robot.rs. IsIntersectRect / IntersectLine / DistLinePoint have no live C++ callers.) |
| matrix_lib/three_g/shadow_stencil.rs    | MatrixLib/3G/src/ShadowStencil.cpp (CVOShadowStencil::Build; GPU passes in matrix_game/shadow.rs `StencilShadowRenderer`) |
| matrix_lib/three_g/texture.rs           | MatrixLib/3G/src/Texture.cpp       |
| matrix_lib/three_g/vector_object.rs     | MatrixLib/3G/src/VectorObject.cpp  |

N/A-by-design (D3D9/Win32 infrastructure replaced by the wgpu glue):
3g (device init/state → gfx/context.rs), BigIB/BigVB (pooled D3D
buffers → wgpu buffers), Cache (resource cache → bundle/loader),
DeviceState (render-state cache → baked pipelines), Form (Win32 window
→ winit glue), Helper (debug-only draw helper, compiled out of
release). ShadowProj (CVOShadowProj) is ported as part of
matrix_game/shadow.rs.

## matrix_game/ (MatrixGame/src/)

| Rust file                     | Original                          |
|-------------------------------|-----------------------------------|
| matrix_game/camera.rs         | MatrixCamera.cpp (strategy/arcade modes + SAutoFlyData FLYCAM autopilot) |
| matrix_game/common.rs         | Common.hpp                        |
| matrix_game/config.rs         | MatrixConfig.cpp (damage/radius/cooldown/overheat tables, cannon props, difficulty, Sounds/ChassisSounds defs; gamma/keybind settings deferred) |
| matrix_game/cursor.rs         | MatrixCursor.cpp (software cursor: Cursors block, frame anim, CalcUV; drawn via interface renderer) |
| matrix_game/form_game.rs      | MatrixFormGame.cpp (+ MatrixGame.cpp entry glue) |
| matrix_game/map.rs            | MatrixMap.cpp (map data + `MapRenderer` draw orchestration + DrawSky/skybox)|
| matrix_game/map_group.rs      | MatrixMapGroup.cpp (BuildBottom + BuildWater — merged across groups by texture; see header) |
| matrix_game/map_prepare.rs    | MatrixMapPrepare.cpp              |
| matrix_game/map_static.rs     | MatrixMapStatic.{cpp,hpp} (base class + ProceedLogic driver + Objects arena) |
| matrix_game/minimap.rs        | MatrixMinimap.cpp                 |
| matrix_game/object.rs         | MatrixObject.cpp (OBJECT_TYPE_MAPOBJECT — decorative rendering + `MapObject` game-object side)|
| matrix_game/object_building.rs| MatrixObjectBuilding.cpp          |
| matrix_game/combat_tests.rs   | (test-only — weapon/damage behavior tests) |
| matrix_game/object_robot.rs   | MatrixObjectRobot.cpp (chassis-only RNeed + per-frame instance sync) |
| matrix_game/logic.rs          | MatrixLogic.cpp (CMatrixMapLogic: Takt driver, Rnd (Park–Miller LCG), Place*/PlaceFindNearReturn/IsAbsenceWall, win/lose CheckStatus; also module root for Logic/ subsystems) |
| matrix_game/map_trace.rs      | MatrixMapTrace.cpp (CMatrixMap::Trace hitscan + FindLocalPath A* + OptimizeMovePath) |
| matrix_game/flyer.rs          | MatrixFlyer.cpp (FO_GIVE_BOT delivery flyers + Damage/HP bar) |
| matrix_game/keybinds.rs       | (no direct peer — user-remappable key translation in front of the form_game match arms) |
| matrix_game/multi_selection.rs| MatrixMultiSelection.cpp (marquee drag-select) |
| matrix_game/object_cannon.rs  | MatrixObjectCannon.cpp (turrets: seek/aim/fire + fire animation, damage, DIP wrecks, renderer) |
| matrix_game/pause_overlay.rs  | (no direct peer — dim quad behind the C++ pause hint) |
| matrix_game/shadow.rs         | MatrixShadowManager.cpp + 3G/ShadowProj.cpp (projected-shadow system) + the CMatrixMap::DrawShadows stencil composition (MatrixMap.cpp:1865 — `StencilShadowRenderer`: volume INCR/DECR pass, proj-in-stencil marks, fullscreen darken quad) |
| matrix_game/slot_marker.rs    | (no direct peer — turret-slot place markers, CreatePlacesShow visuals) |
| matrix_game/progress_bar.rs   | MatrixProgressBar.cpp (3-segment bar + LIC color, atlas-backed) |
| matrix_game/render_pipeline.rs| MatrixRenderPipeline.cpp          |
| matrix_game/side.rs           | MatrixSide.cpp (CMatrixSideUnit data: selection, resources, statistics) |
| matrix_game/side_player.rs    | MatrixSide.cpp player-side logic (TaktPL, PGOrder*, FirePL, underfire recalc, group placement) |
| matrix_game/side_ai.rs        | MatrixSide.cpp enemy-AI logic (TaktHL, TaktTL, WarTL/RepairTL, Regroup, BuildRobot/BuildCannon, ClacSpawnTeam) |
| matrix_game/ter_surface.rs    | MatrixTerSurface.cpp              |
| matrix_game/robot.rs          | MatrixRobot.cpp (CMatrixRobotAI — full order pool (SOrder), spawn/move-out, MoveTo + collision callback + step-aside, GetLost, SBotWeapon fire control / heat / Damage / DOT) |
| matrix_game/sound.rs          | MatrixSoundManager.cpp (CSound/CSoundArray mixer: CalcPanVol, 16-slot eviction, layers, Pos2Key dedup, ttl/fade; voices go to the `SoundOutput` host table — WebAudio in platform/audio_web.rs, rodio in platform/audio_native.rs) |
| matrix_game/water.rs          | MatrixWater.cpp + BuildWater in MatrixMapGroup.cpp + WaterAlpha_t3 in MatrixRenderPipeline.cpp |

N/A-by-design (D3D9 infrastructure replaced by the wgpu glue):
MatrixInstantDraw, MatrixMapTexture, MatrixSampleStateManager,
MatrixSkinManager (vector_object.rs handles materials), MatrixVisiCalc
(frustum culling lives in the renderers),
MatrixLoadProgress / MatrixTransition (index.html spinner + CSS fade),
MatrixDebugInfo + DevConsole (dev/cheat tooling),
MatrixGameDll (SR2-host DLL export shim).
StringConstants: inlined per use.

## matrix_game/effects/ (MatrixGame/src/Effects/)

| Rust file                               | Original                        |
|-----------------------------------------|---------------------------------|
| matrix_game/effects/point_light.rs      | MatrixEffectPointLight.cpp      |
| matrix_game/effects/selection.rs        | MatrixEffectSelection.cpp (animated dot billboards; CBillboard draw-queue + BBT_SELDOT texture deferred — own pipeline + radial-alpha shader stand in) |
| matrix_game/effects/big_boom.rs         | MatrixEffectBigBoom.cpp (blast sweep + geosphere shell) |
| matrix_game/effects/billboard_fx.rs     | MatrixEffectBillboard.cpp (TTL'd billboard-line effects) |
| matrix_game/effects/effects_renderer.rs | (no direct peer — wgpu flush for CBillboard::SortEndDraw + the BBT table of MatrixEffect.cpp InitEffects + CVectorObject draws for bullets/debris) |
| matrix_game/effects/explosion.rs        | MatrixEffectExplosion.cpp (all presets, sparks/intense/fire/mesh debris + FireAnim) |
| matrix_game/effects/fire_plasma.rs      | MatrixEffectFirePlasma.cpp (bolt movement + hit + sprites) |
| matrix_game/effects/flame.rs            | MatrixEffectFlame.cpp (puffs: damage sweep + 10-billboard chains) |
| matrix_game/effects/konus.rs            | MatrixEffectKonus.cpp (cone + splash variants) |
| matrix_game/effects/landscape_spot.rs   | MatrixEffectLandscapeSpot.cpp (voronka / plasma-hit / constant decals) |
| matrix_game/effects/lightening.rs       | MatrixEffectLightening.cpp (bolt + shorted arcs) |
| matrix_game/effects/moving_object.rs    | MatrixEffectMovingObject.cpp (gun/cannon/missile/bomb takts + trails/tracers/meshes) |
| matrix_game/effects/smoke_and_fire.rs   | MatrixEffectSmokeAndFire.cpp (smoke + fire puff emitters) |
| matrix_game/effects/weapon.rs           | MatrixEffectWeapon.{cpp,hpp} (EWeapon + CMatrixEffectWeapon + WeaponHit + CLaser/CVolcano/repair-beam visuals) |
| matrix_game/effects/zahvat.rs           | MatrixEffectZahvat.{cpp,hpp} (capture-progress dot ring)   |
| matrix_game/effects/dust.rs             | MatrixEffectDust.{cpp,hpp} (drifting dust puffs)           |
| matrix_game/effects/elevator_field.rs   | MatrixEffectElevatorField.{cpp,hpp} (flyer tractor-beam helices) |
| matrix_game/effects/move_to.rs          | MatrixEffectMoveTo.{cpp,hpp} (move-order ping)             |

Done-by-design: MatrixEffect base (GameEffect enum + effects_takt +
enforce_limits port the list/limits), Shleif (AddSmoke/AddFire map to
standalone Smoke/Fire spawns at the same call sites), Repair (the
repair-beam visuals live in weapon.rs), Path (no spawn
sites exist in the original — dead code).

## matrix_game/interface/ (MatrixGame/src/Interface/)

| Rust file                               | Original                                           |
|-----------------------------------------|----------------------------------------------------|
| matrix_game/interface/constructor.rs    | CConstructor.{cpp,h} (+ folded CConstructorPanel)  |
| matrix_game/interface/counter.rs        | CCounter.{cpp,h}                                   |
| matrix_game/interface/history.rs        | CHistory.{cpp,h}                                   |
| matrix_game/interface/dialog.rs         | MatrixMap.cpp:3259-3575 dialog mode (menu/win/lose/stat dialogs, confirmations) + MatrixLogic.cpp:2527-2586 stat replacements |
| matrix_game/interface/iface_button.rs   | CIFaceButton.{cpp,h} |
| matrix_game/interface/iface_element.rs  | CIFaceElement.{cpp,h} (+ folded CAnimation.{cpp,h} frame animation) |
| matrix_game/interface/iface_image.rs    | CIFaceImage.{cpp,h} |
| matrix_game/interface/iface_static.rs   | CIFaceStatic.{cpp,h} |
| matrix_game/interface/iface_list.rs     | CInterface.h::CIFaceList (panel container + events + BeginBuildTurret placement state)|
| matrix_game/interface/iface_menu.rs     | CIFaceMenu.{cpp,h}                                 |
| matrix_game/interface/interface.rs      | CInterface.{cpp,h} (panel struct + `if/<Name>` loader) |
| matrix_game/interface/renderer.rs       | (no C++ peer — wgpu 2D quad pipeline for the HUD)  |
| matrix_game/interface/sound.rs          | (no direct peer — queued dispatch for the CSound::Play UI/by-name calls; drained into the mixer by form_game's pump_sounds) |
| matrix_game/interface/hint.rs           | MatrixHint.{cpp,hpp} (template build, chrome, copy-pos button anchors) |
| matrix_game/interface/robot_icons.rs    | CInterface group/personal icon rows |
| matrix_game/interface/text.rs           | (no direct peer — AFT bitmap-font parser + glyph renderer for the SR2 Rangers rasteriser) |


## matrix_game/logic/ (MatrixGame/src/Logic/)

| Rust file                               | Original                        |
|-----------------------------------------|---------------------------------|
| matrix_game/logic/environment.rs        | Logic/MatrixEnvironment.{cpp,h} (CInfo/CEnemy robot sensing) |
| matrix_game/logic/road_network.rs       | Logic/MatrixRoadNetwork.cpp (zones/roads graph, FindPathInZone) |

MatrixAIGroup.cpp's `CMatrixGroup` is ported as the plain-`Vec` side
groups in side.rs / side_player.rs. The MatrixLogicSlot / MatrixRule /
MatrixState / MatrixTactics class family is dead code in the original
(every call site is commented out; the live enemy AI is MatrixSide.cpp
→ side_ai.rs) and is not ported.

## Top-level files

| Rust file   | Original                     |
|-------------|------------------------------|
| lib.rs      | WASM entry (no C++ analogue) |
| main.rs     | native entry (no C++ analogue) |

## gfx/ and platform/ (no C++ counterpart)

| Rust file        | Purpose                                     |
|------------------|---------------------------------------------|
| gfx/bundle.rs    | WASM asset bundle (packaged textures/maps)  |
| gfx/context.rs   | wgpu device + surface setup                 |
| gfx/loader.rs    | Platform-split file loading                 |
| platform/native.rs | Native-specific time / fs                 |
| platform/web.rs  | WASM-specific time / fs                     |
| platform/audio.rs | `SoundOutput` host table (the `g_RangersInterface` sound fn-pointers) |
| platform/audio_web.rs | WebAudio backend (samples from `assets/sounds.bundle`, music playlist) |
| platform/audio_native.rs | rodio backend (samples read from the Rangers `Sound.pkg` / `voices<Lang>.pkg` archives; the soundtrack stays with the host) |
| platform/sound_pkg_map.rs | `Sound.X` resource id → archive file table shared by the native backend and `examples/pack_sounds.rs` |

## shaders/ (WGSL pulled out of the fused renderer files)

The original uses DirectX 9 fixed-function state, not custom shaders.
We need WGSL for wgpu; keeping it next to the Rust that binds it —
not embedded as raw-string constants.

| Shader file                          | Bound by                         |
|--------------------------------------|----------------------------------|
| shaders/minimap.wgsl                 | matrix_game/minimap.rs           |
| shaders/object.wgsl                  | matrix_game/object.rs            |
| shaders/object_building.wgsl         | matrix_game/object_building.rs   |
| shaders/object_shadow.wgsl           | matrix_game/object.rs            |
| shaders/object_shadow_texture.wgsl   | matrix_game/object.rs            |
| shaders/sky_gradient.wgsl            | matrix_game/map.rs (sky)         |
| shaders/sky_skybox.wgsl              | matrix_game/map.rs (sky)         |
| shaders/terrain.wgsl                 | matrix_game/map.rs (MapRenderer) |
| shaders/terrain_gloss.wgsl           | matrix_game/map.rs (MapRenderer) |
| shaders/water.wgsl                   | matrix_game/water.rs             |
| shaders/water_inshore.wgsl           | matrix_game/water.rs             |
| shaders/interface.wgsl               | matrix_game/interface/renderer.rs |
| shaders/progress_bar.wgsl            | matrix_game/progress_bar.rs      |
| shaders/selection_ring.wgsl          | matrix_game/effects/selection.rs |
| shaders/billboard.wgsl               | matrix_game/effects/effects_renderer.rs |
| shaders/effect_mesh.wgsl             | matrix_game/effects/effects_renderer.rs |
| shaders/icon_sharpen.wgsl            | matrix_game/interface/robot_icons.rs (GPU mirror of matrix_lib/bitmap/sharpen.rs) |
| shaders/shadow_stencil.wgsl          | matrix_game/shadow.rs (StencilShadowRenderer) |
