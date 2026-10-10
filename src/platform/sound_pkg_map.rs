//! `Sound.X` resource id → file inside the Rangers sound archives
//! (`DATA/Sound.pkg` for effects, `DATA/voices<Lang>.pkg` for speech).
//! Shared by the web bundle packer (`examples/pack_sounds.rs`) and the
//! native output (`audio_native.rs`).

/// `Sound.X` resource id → file inside SR2's DATA/Sound.pkg. Derived
/// by matching the id names against the SOUND/ROBOTS tree (the
/// authoritative mapping lives in the host's encrypted Main.dat).
/// Speech ids (order voices, base-captured lines, win/lose) have no
/// Sound.pkg file and are absent here.
pub const PKG_MAP: [(&str, &str); 88] = [
    ("Sound.WeapPlasma", "SOUND/ROBOTS/WEAPON/PLASMA.WAV"),
    ("Sound.WeapVolcano", "SOUND/ROBOTS/WEAPON/VOLCANO.WAV"),
    ("Sound.WeapMissile", "SOUND/ROBOTS/WEAPON/MISSILE.WAV"),
    ("Sound.WeapGun", "SOUND/ROBOTS/WEAPON/GUN.WAV"),
    ("Sound.WeapLaser", "SOUND/ROBOTS/WEAPON/LASER.WAV"),
    ("Sound.WeapLight", "SOUND/ROBOTS/WEAPON/LIGHTENING.WAV"),
    ("Sound.WeapRem", "SOUND/ROBOTS/WEAPON/REM.WAV"),
    ("Sound.Flame", "SOUND/ROBOTS/WEAPON/FIRE.WAV"),
    ("Sound.Gorit", "SOUND/ROBOTS/WEAPON/ABLAZE.WAV"),
    ("Sound.Zamknulo", "SOUND/ROBOTS/WEAPON/SHORTED.WAV"),
    ("Sound.Splash", "SOUND/ROBOTS/WORLD/SPLASH.WAV"),
    ("Sound.Rhit", "SOUND/ROBOTS/HIT/RHIT.WAV"),
    ("Sound.expl1", "SOUND/EXPL/EXPL1.WAV"),
    ("Sound.ExpMiss", "SOUND/ROBOTS/EXP/EXPMISS.WAV"),
    ("Sound.ExpRobot", "SOUND/ROBOTS/EXP/ROBOT.WAV"),
    ("Sound.ExpObj", "SOUND/ROBOTS/EXP/OBJECT.WAV"),
    ("Sound.ExpBuild", "SOUND/ROBOTS/EXP/BUILDING.WAV"),
    ("Sound.ExpBuildL", "SOUND/ROBOTS/EXP/BUILDING2.WAV"),
    ("Sound.ExpBuildS", "SOUND/ROBOTS/EXP/SMALLBUILD.WAV"),
    ("Sound.ExpBuildS2", "SOUND/ROBOTS/EXP/SMALLBUILD2.WAV"),
    ("Sound.BaseDoorOpen", "SOUND/ROBOTS/BASE/BASE_DOOR_OPEN.WAV"),
    ("Sound.BaseDoorClose", "SOUND/ROBOTS/BASE/BASE_DOOR_CLOSE.WAV"),
    ("Sound.BasePlatformUp", "SOUND/ROBOTS/BASE/BASE_PLATFORM_UP.WAV"),
    ("Sound.BasePlatformDown", "SOUND/ROBOTS/BASE/BASE_PLATFORM_DOWN.WAV"),
    ("Sound.BasePlatformUpStop", "SOUND/ROBOTS/BASE/BASE_PLATFORM_UP_STOP.WAV"),
    ("Sound.BaseAmbient", "SOUND/ROBOTS/BASE/BASE_AMB.WAV"),
    ("Sound.ChasPne", "SOUND/ROBOTS/CHASSIS/PNEUMATIC.WAV"),
    ("Sound.ChasWheel", "SOUND/ROBOTS/CHASSIS/WHEEL.WAV"),
    ("Sound.ChasTrack", "SOUND/ROBOTS/CHASSIS/TRACK.WAV"),
    ("Sound.ChasHover", "SOUND/ROBOTS/CHASSIS/HOVERCRAFT.WAV"),
    ("Sound.ChasAnti", "SOUND/ROBOTS/CHASSIS/ANTIGRAVITY.WAV"),
    ("Sound.HullUniv", "SOUND/ROBOTS/HULL/HULL.WAV"),
    ("Sound.Fall", "SOUND/ROBOTS/FALL.WAV"),
    ("Sound.Mute", "SOUND/ROBOTS/MUTE.WAV"),
    ("Sound.Nini", "SOUND/ROBOTS/NINI.WAV"),
    ("Sound.Resurs", "SOUND/ROBOTS/RESURS.WAV"),
    ("Sound.TPain1", "SOUND/ROBOTS/TERRON/PAIN1.WAV"),
    ("Sound.TPain2", "SOUND/ROBOTS/TERRON/PAIN2.WAV"),
    ("Sound.TPain3", "SOUND/ROBOTS/TERRON/PAIN3.WAV"),
    ("Sound.TerronDead", "SOUND/ROBOTS/TERRON/TERRONDEAD.WAV"),
    ("Sound.Stand", "SOUND/ROBOTS/TURELL/STAND.WAV"),
    ("Sound.VertStart", "SOUND/ROBOTS/VERTOLET/START.WAV"),
    ("Sound.VertLoop", "SOUND/ROBOTS/VERTOLET/LOOP.WAV"),
    ("Sound.DOpen", "SOUND/ROBOTS/MAP/DOOROPEN.WAV"),
    ("Sound.DClose", "SOUND/ROBOTS/MAP/DOORCLOSE.WAV"),
    ("Sound.ButtonClick", "SOUND/ROBOTS/CLICKS/CLICK.WAV"),
    ("Sound.Rclick1", "SOUND/ROBOTS/VOICES/ROBOTS/CLICK.WAV"),
    ("Sound.Rclick2", "SOUND/ROBOTS/VOICES/ROBOTS/CLICK2.WAV"),
    ("Sound.Plus", "SOUND/ROBOTS/CLICKS/PLUS.WAV"),
    ("Sound.Minus", "SOUND/ROBOTS/CLICKS/MINUS.WAV"),
    ("Sound.MapWater", "SOUND/ROBOTS/MAP/WATER.WAV"),
    // ── Speech (voicesRus.pkg — the localized announcer/robot voices) ──
    ("Sound.GetBase", "SOUND/ROBOTS/VOICES/BASE/BAZAPROTIVNIKA.WAV"),
    ("Sound.UnderAttack0", "SOUND/ROBOTS/VOICES/BASE/NANASNAPALI.WAV"),
    ("Sound.UnderAttack1", "SOUND/ROBOTS/VOICES/BASE/NASATAKUUT.WAV"),
    ("Sound.GetOurBase", "SOUND/ROBOTS/VOICES/BASE/NASHABAZA.WAV"),
    ("Sound.KillOurBase", "SOUND/ROBOTS/VOICES/BASE/NASHABAZAUNICH.WAV"),
    ("Sound.KillOurFactory", "SOUND/ROBOTS/VOICES/BASE/NASHZAVODUNICH.WAV"),
    ("Sound.GetOurFactory", "SOUND/ROBOTS/VOICES/BASE/NASHZAVODZAHV.WAV"),
    ("Sound.KillOurBuilding", "SOUND/ROBOTS/VOICES/BASE/VRAGUNICHTOJILNASHE.WAV"),
    ("Sound.GetFactory", "SOUND/ROBOTS/VOICES/BASE/ZAHVACHENZAVODPRO.WAV"),
    ("Sound.HelpApp", "SOUND/ROBOTS/VOICES/HELPAPPROACHING.WAV"),
    ("Sound.Help", "SOUND/ROBOTS/VOICES/MAINTAINCE.WAV"),
    ("Sound.RWin", "SOUND/ROBOTS/VOICES/WIN.WAV"),
    ("Sound.RLoose", "SOUND/ROBOTS/VOICES/LOOSE.WAV"),
    ("Sound.Armor", "SOUND/ROBOTS/VOICES/ROBOTS/ARMOR.WAV"),
    ("Sound.Attack", "SOUND/ROBOTS/VOICES/ROBOTS/ATTACK.WAV"),
    ("Sound.Capture", "SOUND/ROBOTS/VOICES/ROBOTS/CAPTURE.WAV"),
    ("Sound.ArmorProg", "SOUND/ROBOTS/VOICES/ROBOTS/PROGARMOR.WAV"),
    ("Sound.AttackProg", "SOUND/ROBOTS/VOICES/ROBOTS/PROGATTACK.WAV"),
    ("Sound.CaptureProg", "SOUND/ROBOTS/VOICES/ROBOTS/PROGCAPTURE.WAV"),
    ("Sound.Patrul", "SOUND/ROBOTS/VOICES/ROBOTS/GO.WAV"),
    ("Sound.RReady", "SOUND/ROBOTS/VOICES/ROBOTS/ROBOTDONE.WAV"),
    ("Sound.RReadyA", "SOUND/ROBOTS/VOICES/ROBOTS/ROBOTDONE1.WAV"),
    ("Sound.BReady", "SOUND/ROBOTS/VOICES/ROBOTS/ROBOTGOTOV.WAV"),
    ("Sound.GoPneumatic0", "SOUND/ROBOTS/VOICES/ROBOTS/CHASSIS/PNEUMATIC.WAV"),
    ("Sound.GoPneumatic1", "SOUND/ROBOTS/VOICES/ROBOTS/CHASSIS/PNEUMATIC2.WAV"),
    ("Sound.GoWheel0", "SOUND/ROBOTS/VOICES/ROBOTS/CHASSIS/WHEEL.WAV"),
    ("Sound.GoWheel1", "SOUND/ROBOTS/VOICES/ROBOTS/CHASSIS/WHEEL2.WAV"),
    ("Sound.GoTrack0", "SOUND/ROBOTS/VOICES/ROBOTS/CHASSIS/TRACK.WAV"),
    ("Sound.GoTrack1", "SOUND/ROBOTS/VOICES/ROBOTS/CHASSIS/TRACK2.WAV"),
    ("Sound.GoHovercraft0", "SOUND/ROBOTS/VOICES/ROBOTS/CHASSIS/HOVERCRAFT.WAV"),
    ("Sound.GoHovercraft1", "SOUND/ROBOTS/VOICES/ROBOTS/CHASSIS/HOVERCRAFT2.WAV"),
    ("Sound.GoAntigravi0", "SOUND/ROBOTS/VOICES/ROBOTS/CHASSIS/ANTIGRAVITY.WAV"),
    ("Sound.GoAntigravi1", "SOUND/ROBOTS/VOICES/ROBOTS/CHASSIS/ANTIGRAVITY2.WAV"),
    // E/H/L/R turret-built voices (t_build_0..3).
    ("Sound.ETurel", "SOUND/ROBOTS/VOICES/TURELS/EASYWEAPON.WAV"),
    ("Sound.HTurel", "SOUND/ROBOTS/VOICES/TURELS/HARDWEAPON.WAV"),
    ("Sound.LTurel", "SOUND/ROBOTS/VOICES/TURELS/LAZERWEAPON.WAV"),
    ("Sound.RTurel", "SOUND/ROBOTS/VOICES/TURELS/ROCKETWEAPON.WAV"),
];

/// Ambience ids beyond the fixed table — AMB_LOW1..4 for MapAmb1..4,
/// FANTASY/BOTTLE for the rest (best-guess; all are quiet loops).
pub fn pkg_map_extra(path: &str) -> Option<&'static str> {
    Some(match path {
        "Sound.MapAmb1" => "SOUND/ROBOTS/MAP/AMB_LOW1.WAV",
        "Sound.MapAmb2" => "SOUND/ROBOTS/MAP/AMB_LOW2.WAV",
        "Sound.MapAmb3" => "SOUND/ROBOTS/MAP/AMB_LOW3.WAV",
        "Sound.MapAmb4" => "SOUND/ROBOTS/MAP/AMB_LOW4.WAV",
        "Sound.MapAmb5" => "SOUND/ROBOTS/MAP/FANTASY1.WAV",
        "Sound.MapAmb6" => "SOUND/ROBOTS/MAP/FANTASY2.WAV",
        "Sound.MapAmb7" => "SOUND/ROBOTS/MAP/BOTTLE.WAV",
        "Sound.HangarOpen" => "SOUND/HANGAROPEN.WAV",
        "Sound.FormShipOpen" => "SOUND/FORMSHIPOPEN.WAV",
        "Sound.FormShipClose" => "SOUND/FORMSHIPCLOSE.WAV",
        "Sound.FormPlanetNone1" => "SOUND/PLANET/NONE/1.WAV",
        "Sound.FormPlanetNone3" => "SOUND/PLANET/NONE/3.WAV",
        _ => return None,
    })
}

/// Archive file for a `Sound.X` resource id (case-insensitive).
pub fn pkg_file_for(path: &str) -> Option<&'static str> {
    PKG_MAP
        .iter()
        .find(|(id, _)| id.eq_ignore_ascii_case(path))
        .map(|(_, f)| *f)
        .or_else(|| pkg_map_extra(path))
}
