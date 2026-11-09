//! Terrain features and surface layouts (port of `Terrain.gs`).
//!
//! Catalog entry fields:
//!   geo    dry | mid | wet | any   which geospheres it suits
//!   tags   WATER needs liquid surface water (not Burning or Desert worlds)
//!          BIO   implies visible life (forces native life, or is rerolled)
//!          MOON  describes the body's own moons (not on moons; adds moons)
//!          MOON_ONLY  only makes sense on a moon of a gas giant
//!          CLOUD needs a real atmosphere (not on Thin)
//!          LAND  needs dry land (not on Water Worlds)
//!   temps  allowed temperatures (default: all but Frozen)

use super::dice;
use crate::sim::planet::{AtmoKind, Geosphere, TempBand};
use rand::rngs::StdRng;

pub const WATER: u8 = 1;
pub const BIO: u8 = 2;
pub const MOON: u8 = 4;
pub const MOON_ONLY: u8 = 8;
pub const CLOUD: u8 = 16;
pub const LAND: u8 = 32;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Geo {
    Dry,
    Mid,
    Wet,
    Any,
}

pub struct Entry {
    pub name: &'static str,
    pub geo: Geo,
    pub tags: u8,
    pub temps: Option<&'static [TempBand]>,
}

/// What the terrain rules need to know about a body.
#[derive(Clone, Copy)]
pub struct Ctx {
    pub temp: TempBand,
    pub geo: Option<Geosphere>,
    pub atmo: Option<AtmoKind>,
    pub is_moon: bool,
    pub life_eligible: bool,
}

use TempBand::*;
const COOL: &[TempBand] = &[Cold, Temperate];
const MILD: &[TempBand] = &[Cold, Temperate, Hot];
const WARM: &[TempBand] = &[Temperate, Hot];
const HOT: &[TempBand] = &[Hot, Burning];
const COLD: &[TempBand] = &[Cold];
const BURN: &[TempBand] = &[Burning];
const HOTONLY: &[TempBand] = &[Hot];

const fn t(name: &'static str, geo: Geo, tags: u8, temps: Option<&'static [TempBand]>) -> Entry {
    Entry { name, geo, tags, temps }
}

use Geo::{Any, Dry, Mid, Wet};

pub static CATALOG: &[Entry] = &[
    // --- Classic terrain ---
    t("Huge impact crater", Any, 0, None),
    t("Plains of silicon glass", Dry, 0, None),
    t("Disturbing wind-cut rock formations", Dry, LAND, None),
    t("Permanent global dust-storm", Dry, LAND, None),
    t("Eerily colored dust plains", Dry, LAND, None),
    t("Active volcanic lava fields", Any, LAND, None),
    t("Extensive salt flats", Dry, LAND, None),
    t("Dust-laden, permanent sunset sky", Dry, LAND, None),
    t("Ancient, blackened lava plains", Dry, LAND, None),
    t("Thermal springs and steam vents", Mid, 0, None),
    t("Tall, gravel-strewn mountains", Dry, LAND, None),
    t("Howling winds that never stop", Any, 0, None),
    t("Daily fog banks roll in", Mid, WATER | CLOUD, None),
    t("Deep and wide rift valleys", Any, 0, None),
    t("Bizarrely eroded, wind-cut badlands", Dry, LAND, None),
    t("Steep-sided river gorges cut into soft rocks", Mid, WATER | LAND, None),
    t("Huge moon dominates day/night sky", Any, MOON, None),
    t("World-spanning super canyon", Any, LAND, None),
    t("Impressive river of great length", Mid, WATER | LAND, None),
    t("Oddly colored forests of alien vegetation", Mid, BIO | LAND, None),
    t("Mountains cut by sky-blue lakes", Mid, WATER | LAND, Some(COOL)),
    t("Sweeping plains of elephant grass", Mid, BIO | LAND, None),
    t("Highly toxic, but beautiful, plant-life", Mid, BIO, None),
    t("Small, bright, incredibly fast moons in orbit", Any, MOON, None),
    t("Vast and complex river delta", Wet, WATER, None),
    t("Immense series of waterfalls", Mid, WATER | LAND, None),
    t("Endless mudflats with twisting waterways", Wet, WATER, None),
    t("Impressive coastline of fjords and cliffs", Wet, WATER | LAND, Some(COOL)),
    t("Volcanoes, active & widespread", Any, 0, None),
    t("Impenetrable jungle", Wet, BIO | WATER | LAND, Some(WARM)),
    t("Dangerous tides -- fast and loud", Wet, WATER, None),
    t("Vast, permanent super storm", Any, CLOUD, None),
    t("Toxic sea creatures floating with the currents", Wet, BIO | WATER, None),
    t("Volcanic island chains", Wet, WATER, None),
    t("Permanently overcast with unrelenting rainfall", Wet, WATER | CLOUD, None),
    t("Mildly acidic oceans and rainfall", Wet, WATER | CLOUD, None),
    // --- Mixed and Earth-like landscapes ---
    t("Rolling hills and broad river valleys", Mid, WATER | LAND, Some(MILD)),
    t("Snow-capped mountain ranges above green lowlands", Mid, WATER | LAND, Some(COOL)),
    t("Great inland sea ringed by mountains", Mid, WATER | LAND, None),
    t("Highland plateaus cut by deep canyons", Mid, LAND, None),
    t("A river system that drains half a continent", Mid, WATER | LAND, None),
    t("Chains of crater lakes", Mid, WATER | LAND, None),
    t("Glacier-carved valleys", Mid, WATER | LAND, Some(COOL)),
    t("Endless wetlands and peat bogs", Wet, WATER | BIO | LAND, Some(MILD)),
    t("Shallow, sun-warmed lagoons", Wet, WATER, Some(WARM)),
    t("Massive tidal flats exposed twice a day", Wet, WATER, None),
    t("Towering sea stacks and eroded coastal arches", Wet, WATER | LAND, None),
    t("Monsoon-lashed coastal lowlands", Wet, WATER | CLOUD | LAND, Some(WARM)),
    t("Karst country of sinkholes and flooded caverns", Mid, WATER | LAND, None),
    t("Mesas and buttes of banded red rock", Dry, LAND, None),
    t("Obsidian fields from ancient eruptions", Dry, LAND, None),
    t("Crystal formations jutting from the ground", Any, LAND, None),
    t("Worn-down stumps of ancient mountains", Any, LAND, None),
    t("Meteor-scarred highlands", Any, LAND, None),
    t("Lava tubes and collapsed caverns", Any, LAND, None),
    t("Geothermal valleys of boiling mud", Mid, LAND, None),
    t("Aurora-lit polar regions", Any, 0, None),
    t("Electrical storms that never cease", Any, CLOUD, None),
    t("Ash-darkened skies from constant eruptions", Any, CLOUD, None),
    t("Dunes the size of mountains", Dry, LAND, None),
    t("Dry seabeds of a vanished ocean", Dry, LAND, None),
    t("Oases strung along an ancient fault line", Dry, WATER | LAND, Some(WARM)),
    // --- Living landscapes (need visible life) ---
    t("Towering forests of giant plant life", Mid, BIO | LAND, Some(MILD)),
    t("Savanna dotted with lone giant trees", Mid, BIO | LAND, Some(WARM)),
    t("Wind-swept scrub steppe", Dry, BIO | LAND, Some(MILD)),
    t("Reef shelves stretching for thousands of kilometres", Wet, BIO | WATER, Some(WARM)),
    t("Mangrove-like coastal swamps", Wet, BIO | WATER | LAND, Some(WARM)),
    t("Floating rafts of vegetation on calm seas", Wet, BIO | WATER, None),
    t("Bioluminescent coastal waters", Wet, BIO | WATER, None),
    t("Fungal forests in perpetual mist", Mid, BIO | CLOUD | LAND, Some(MILD)),
    t("Kelp-like forests in shallow seas", Wet, BIO | WATER, Some(MILD)),
    // --- Cold worlds ---
    t("Polar ice caps larger than continents", Any, 0, Some(COLD)),
    t("Tundra plains over permafrost", Mid, LAND, Some(COLD)),
    t("Frozen seas cracked by pressure ridges", Wet, 0, Some(COLD)),
    t("Retreating ice sheets uncovering new land", Mid, LAND, Some(COLD)),
    t("Blizzards that last for months", Any, CLOUD, Some(COLD)),
    // --- Hot and burning worlds ---
    t("Sulphur plains and acid lakes", Any, LAND, Some(HOT)),
    t("Glass-smooth lava seas, recently cooled", Any, LAND, Some(HOT)),
    t("Molten lowlands glowing through the night", Any, LAND, Some(BURN)),
    t("Crushing, cloud-choked sky over scorched rock", Any, CLOUD | LAND, Some(BURN)),
    t("Heat-shimmering salt deserts", Dry, LAND, Some(HOT)),
    t("Boiling seas venting steam into the sky", Wet, CLOUD, Some(HOTONLY)),
    // --- Moons of gas giants ---
    t("The gas giant fills half the sky", Any, MOON_ONLY, None),
    t("Tidally heated volcanic plains", Any, MOON_ONLY | LAND, None),
    t("Plains streaked with ejecta from the gas giant's other moons", Any, MOON_ONLY | LAND, None),
];

/// Frozen worlds: ice planets, ice moons and frozen terrestrial planets.
pub static ICE_CATALOG: &[&str] = &[
    "Huge impact crater",
    "Geysers spew water into low orbit from long fissures",
    "Deep fissures leading to a subsurface ocean",
    "Dramatically colored blue-green ice fissures",
    "Huge and active cryovolcano",
    "Vast range of ice mountains",
    "World-spanning super canyon",
    "Disturbing, wind-cut ice formations",
    "Black, dust-covered ice plains",
    "Impressive ice escarpment of great length",
    "Extensive dune-fields of methane sand grains",
    "Nitrogen ice glaciers creeping across the plains",
    "Smooth ice plains over a hidden ocean",
    "Fields of jagged ice spires",
    "Dark, organic-stained ice",
    "Frozen methane lakes",
    "An ancient continent buried under kilometres of ice",
    "Polygonal cracked-ice terrain",
    "Wind-polished ice deserts",
    "Crevasse fields kilometres deep",
    "Frost-rimmed impact basins",
];

/// Ice terrain that implies liquid water below (used by the life rules).
pub fn subsurface_ocean(terrain: &[String]) -> bool {
    const HIDDEN: [&str; 4] = [
        "Deep fissures leading to a subsurface ocean",
        "Geysers spew water into low orbit from long fissures",
        "Huge and active cryovolcano",
        "Smooth ice plains over a hidden ocean",
    ];
    terrain.iter().any(|t| HIDDEN.contains(&t.as_str()))
}

pub fn entry(name: &str) -> Option<&'static Entry> {
    CATALOG.iter().find(|e| e.name == name)
}

/// True if ANY feature has the tag.
pub fn has_tag(terrain: &[String], tag: u8) -> bool {
    terrain.iter().any(|t| entry(t).is_some_and(|e| e.tags & tag != 0))
}

pub fn feature_count(ctx: &Ctx) -> usize {
    if ctx.is_moon {
        1
    } else if ctx.temp == Temperate {
        3
    } else {
        2
    }
}

fn geo_weight(ctx: &Ctx, e: &Entry) -> f64 {
    use Geosphere as G;
    let (dry, mid, wet, any) = match ctx.geo {
        Some(G::Desert) => (4.0, 0.0, 0.0, 1.5),
        Some(G::Arid) => (3.0, 1.5, 0.0, 1.5),
        Some(G::TemperateDry) => (1.5, 3.0, 0.5, 1.5),
        Some(G::TemperateWet) => (0.3, 3.0, 2.0, 1.5),
        Some(G::Wet) => (0.0, 1.5, 4.0, 1.5),
        Some(G::Water) => (0.0, 0.0, 4.0, 1.0),
        None => (2.0, 1.0, 0.5, 2.0),
    };
    let mut w = match e.geo {
        Dry => dry,
        Mid => mid,
        Wet => wet,
        Any => any,
    };
    if e.temps.is_some_and(|t| t.len() <= 2) {
        w *= 2.0;
    }
    if e.tags & BIO != 0 {
        w *= 0.35;
    }
    if e.name == "Huge impact crater" && ctx.temp == Temperate {
        w *= 0.4;
    }
    w
}

pub fn allowed(ctx: &Ctx, e: &Entry, allow_bio: bool) -> bool {
    let temps = e.temps.unwrap_or(&[Cold, Temperate, Hot, Burning]);
    if !temps.contains(&ctx.temp) {
        return false;
    }
    if e.tags & WATER != 0 && (ctx.temp == Burning || ctx.geo == Some(Geosphere::Desert)) {
        return false;
    }
    if e.tags & CLOUD != 0 && ctx.atmo == Some(AtmoKind::Thin) {
        return false;
    }
    if e.tags & LAND != 0 && ctx.geo == Some(Geosphere::Water) {
        return false;
    }
    if e.tags & MOON != 0 && ctx.is_moon {
        return false;
    }
    if e.tags & MOON_ONLY != 0 && !ctx.is_moon {
        return false;
    }
    if e.tags & BIO != 0 && (!allow_bio || !ctx.life_eligible) {
        return false;
    }
    geo_weight(ctx, e) > 0.0
}

/// Distinctive features for a non-frozen world.
pub fn pick_features(rng: &mut StdRng, ctx: &Ctx, count: usize, allow_bio: bool) -> Vec<String> {
    let mut chosen: Vec<&'static str> = Vec::new();
    let mut has_moon = false;
    for _ in 0..count {
        let cands: Vec<(&'static str, f64)> = CATALOG
            .iter()
            .filter(|e| allowed(ctx, e, allow_bio))
            .filter(|e| !chosen.contains(&e.name))
            .filter(|e| !(has_moon && e.tags & (MOON | MOON_ONLY) != 0))
            .map(|e| (e.name, geo_weight(ctx, e)))
            .collect();
        if cands.is_empty() {
            break;
        }
        let p = dice::weighted(rng, &cands);
        if entry(p).is_some_and(|e| e.tags & (MOON | MOON_ONLY) != 0) {
            has_moon = true;
        }
        chosen.push(p);
    }
    if chosen.is_empty() {
        chosen.push("Huge impact crater");
    }
    chosen.into_iter().map(String::from).collect()
}

/// One replacement feature, avoiding what the body already has.
pub fn pick_replacement(rng: &mut StdRng, ctx: &Ctx, existing: &[String], allow_bio: bool) -> String {
    let cands: Vec<(&'static str, f64)> = CATALOG
        .iter()
        .filter(|e| !existing.iter().any(|x| x == e.name) && e.tags & MOON == 0 && allowed(ctx, e, allow_bio))
        .map(|e| (e.name, geo_weight(ctx, e)))
        .collect();
    if cands.is_empty() {
        "Huge impact crater".into()
    } else {
        dice::weighted(rng, &cands).into()
    }
}

pub fn ice_features(rng: &mut StdRng, count: usize) -> Vec<String> {
    dice::distinct(rng, ICE_CATALOG, count).into_iter().map(String::from).collect()
}

/// The big picture of a world's geography. `ice_world` = Ice Planet.
pub fn surface_layout(rng: &mut StdRng, ctx: &Ctx, ice_world: bool) -> String {
    if ctx.is_moon {
        return String::new();
    }
    let pick = |rng: &mut StdRng, v: &[&str]| dice::pick(rng, v).to_string();
    if ice_world {
        return pick(rng, &[
            "Ice shell over a deep subsurface ocean",
            "Rock-and-ice world of frozen plains",
            "Cratered ice crust, ancient and unchanging",
            "Patchwork of dark rock and bright ice fields",
        ]);
    }
    if ctx.temp == Frozen {
        let wet = ctx.geo.is_some_and(|g| g.is_wet());
        return if wet {
            pick(rng, &[
                "Frozen ocean world under a thick ice shell",
                "Ice-covered continents locked in frozen seas",
                "Snowball world: ice from pole to pole",
            ])
        } else {
            pick(rng, &[
                "Barren frozen rock with polar ice",
                "Frozen highlands and ice-filled basins",
                "Snowball world: ice from pole to pole",
            ])
        };
    }
    if ctx.temp == Burning {
        let thick = matches!(ctx.atmo, Some(AtmoKind::Dense | AtmoKind::Corrosive | AtmoKind::Infiltrating));
        return if thick {
            pick(rng, &[
                "Volcanic plains under a crushing, furnace-hot atmosphere",
                "Scorched highlands above molten lowlands",
            ])
        } else {
            pick(rng, &[
                "Scorched, cratered rock",
                "Baked barren highlands and glassy plains",
                "Scorched highlands above molten lowlands",
            ])
        };
    }
    use Geosphere as G;
    type L = (&'static str, f64, Option<&'static [TempBand]>);
    let table: &[L] = match ctx.geo {
        Some(G::Desert) => &[
            ("Endless sand seas", 3.0, None),
            ("Rocky barrens and mesas", 3.0, None),
            ("Cracked salt-pan basins", 2.0, None),
            ("Dry ocean basins of a vanished sea", 2.0, None),
            ("Dune seas around a few oasis basins", 1.0, Some(WARM)),
        ],
        Some(G::Arid) => &[
            ("Vast deserts with scattered inland seas", 3.0, None),
            ("Arid continents threaded by a few great rivers", 3.0, None),
            ("Dry highlands around a small shallow sea", 2.0, None),
            ("Steppe and scrubland continents", 2.0, Some(COOL)),
            ("Sun-baked continents with shrinking seas", 2.0, Some(HOTONLY)),
        ],
        Some(G::TemperateDry) => &[
            ("Large continents with small, shallow seas", 3.0, None),
            ("One supercontinent with a dry interior", 3.0, None),
            ("Mixed terrain: plains, mountains and inland seas", 3.0, None),
            ("Highland plateaus cut by river valleys", 2.0, None),
            ("Ice-age continents with glaciers reaching the mid-latitudes", 2.0, Some(COLD)),
        ],
        Some(G::TemperateWet) => &[
            ("Earth-like: large continents and oceans", 4.0, None),
            ("Several continents separated by wide oceans", 3.0, None),
            ("Mixed terrain: forests, plains, mountains and seas", 3.0, None),
            ("Two great continents and a world ocean", 2.0, None),
            ("Continents fringed with archipelagos", 2.0, None),
            ("Ice-capped continents and cold, stormy oceans", 2.0, Some(COLD)),
            ("Hot continents ringed by warm shallow seas", 2.0, Some(HOTONLY)),
        ],
        Some(G::Wet) => &[
            ("Archipelago world of thousands of islands", 4.0, None),
            ("Small continents in a global ocean", 3.0, None),
            ("Swamps, deltas and shallow seas", 2.0, None),
            ("Island chains along volcanic arcs", 2.0, None),
            ("One island-continent in a world ocean", 1.0, None),
        ],
        Some(G::Water) => &[
            ("Global ocean broken only by scattered islands", 3.0, None),
            ("Planet-wide ocean over drowned continents", 3.0, None),
            ("Deep global ocean with a few volcanic island arcs", 2.0, None),
            ("Shallow world-sea over sunken plains", 2.0, None),
        ],
        None => &[],
    };
    let fitting: Vec<(&'static str, f64)> = table
        .iter()
        .filter(|(_, _, temps)| temps.is_none_or(|t| t.contains(&ctx.temp)))
        .map(|(v, w, _)| (*v, *w))
        .collect();
    if fitting.is_empty() { String::new() } else { dice::weighted(rng, &fitting).to_string() }
}
