//! Bodies in a star system — planets, moons, ice worlds and asteroid belts —
//! and every trait the world generator gives them. Generation itself lives
//! in [`super::worldgen`]; this module is the data plus small helpers.

use super::res::Res;
use serde::{Deserialize, Serialize};

/// Gameplay class: decides how the scoop and the drones treat a body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanetClass {
    Terrestrial,
    SuperEarth,
    GasGiant,
    /// Small gas giant: both scoopable (its envelope) and mineable (its ices).
    IceGiant,
    /// Small frozen world of rock and ice.
    IceWorld,
    AsteroidBelt,
}

impl PlanetClass {
    pub fn name(&self) -> &'static str {
        match self {
            PlanetClass::Terrestrial => "Terrestrial",
            PlanetClass::SuperEarth => "Super-Earth",
            PlanetClass::GasGiant => "Gas giant",
            PlanetClass::IceGiant => "Ice giant",
            PlanetClass::IceWorld => "Ice world",
            PlanetClass::AsteroidBelt => "Asteroid belt",
        }
    }

    /// Whether drones can mine solid deposits here.
    pub fn mineable(&self) -> bool {
        !matches!(self, PlanetClass::GasGiant)
    }

    /// Whether the atmospheric scoop can work here (gas/ice giants).
    pub fn scoopable(&self) -> bool {
        matches!(self, PlanetClass::GasGiant | PlanetClass::IceGiant)
    }
}

/// Which star a body orbits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Host {
    /// The primary star.
    A,
    /// The companion star (S-type binaries).
    B,
    /// Both stars (P-type, circumbinary).
    AB,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AtmoKind {
    Thin,
    Breathable,
    Toxic,
    Dense,
    Corrosive,
    Infiltrating,
}

impl AtmoKind {
    pub fn name(&self) -> &'static str {
        match self {
            AtmoKind::Thin => "Thin",
            AtmoKind::Breathable => "Breathable",
            AtmoKind::Toxic => "Toxic",
            AtmoKind::Dense => "Dense",
            AtmoKind::Corrosive => "Corrosive",
            AtmoKind::Infiltrating => "Infiltrating",
        }
    }

    pub fn describe(&self) -> &'static str {
        match self {
            AtmoKind::Thin => "Thin — too little air to breathe or hold heat",
            AtmoKind::Breathable => "Breathable — humans can walk unmasked",
            AtmoKind::Toxic => "Toxic — needs a breather; skin-safe",
            AtmoKind::Dense => "Dense — crushing pressure, holds heat",
            AtmoKind::Corrosive => "Corrosive — eats seals and hull plating",
            AtmoKind::Infiltrating => "Infiltrating — gets through any seal; worst there is",
        }
    }

    /// Greenhouse warming (K) on top of the bare equilibrium temperature.
    pub fn greenhouse_k(&self) -> f64 {
        match self {
            AtmoKind::Thin => -5.0,
            AtmoKind::Breathable => 12.0,
            AtmoKind::Toxic => 18.0,
            AtmoKind::Dense => 45.0,
            AtmoKind::Corrosive | AtmoKind::Infiltrating => 120.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TempBand {
    Frozen,
    Cold,
    Temperate,
    Hot,
    Burning,
}

impl TempBand {
    pub fn name(&self) -> &'static str {
        match self {
            TempBand::Frozen => "Frozen",
            TempBand::Cold => "Cold",
            TempBand::Temperate => "Temperate",
            TempBand::Hot => "Hot",
            TempBand::Burning => "Burning",
        }
    }

    pub fn range(&self) -> &'static str {
        match self {
            TempBand::Frozen => "−50 °C or less",
            TempBand::Cold => "−50 to 0 °C",
            TempBand::Temperate => "0 to 30 °C",
            TempBand::Hot => "30 to 80 °C",
            TempBand::Burning => "80 °C or more",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Geosphere {
    Desert,
    Arid,
    TemperateDry,
    TemperateWet,
    Wet,
    Water,
}

impl Geosphere {
    pub fn name(&self) -> &'static str {
        match self {
            Geosphere::Desert => "Desert World",
            Geosphere::Arid => "Arid World",
            Geosphere::TemperateDry => "Temperate-Dry World",
            Geosphere::TemperateWet => "Temperate-Wet World",
            Geosphere::Wet => "Wet World",
            Geosphere::Water => "Water World",
        }
    }

    /// Fraction of the surface under water (drives rendering and ice).
    pub fn sea_fraction(&self) -> f32 {
        match self {
            Geosphere::Desert => 0.0,
            Geosphere::Arid => 0.15,
            Geosphere::TemperateDry => 0.35,
            Geosphere::TemperateWet => 0.6,
            Geosphere::Wet => 0.8,
            Geosphere::Water => 0.97,
        }
    }

    pub fn is_wet(&self) -> bool {
        matches!(self, Geosphere::TemperateWet | Geosphere::Wet | Geosphere::Water)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GgFeature {
    Storms,
    SingleSuperStorm,
    Rings,
    HighWinds,
    IntenseRadiation,
    Small,
}

impl GgFeature {
    pub const ALL: [GgFeature; 6] = [
        GgFeature::Storms,
        GgFeature::SingleSuperStorm,
        GgFeature::Rings,
        GgFeature::HighWinds,
        GgFeature::IntenseRadiation,
        GgFeature::Small,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            GgFeature::Storms => "Storms",
            GgFeature::SingleSuperStorm => "Single super storm",
            GgFeature::Rings => "Rings",
            GgFeature::HighWinds => "High winds",
            GgFeature::IntenseRadiation => "Intense radiation fields",
            GgFeature::Small => "Small gas giant",
        }
    }

    /// Multiplier on scoop yield in this envelope.
    pub fn scoop_mult(&self) -> f64 {
        match self {
            GgFeature::SingleSuperStorm => 0.7,
            GgFeature::HighWinds => 0.85,
            GgFeature::Storms => 0.95,
            _ => 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BeltFeature {
    Bright,
    HighInclination,
    Dust,
    DwarfPlanets,
    Wide,
    MineralRich,
}

impl BeltFeature {
    pub const ALL: [BeltFeature; 6] = [
        BeltFeature::Bright,
        BeltFeature::HighInclination,
        BeltFeature::Dust,
        BeltFeature::DwarfPlanets,
        BeltFeature::Wide,
        BeltFeature::MineralRich,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            BeltFeature::Bright => "Bright and highly visible",
            BeltFeature::HighInclination => "High orbital inclination",
            BeltFeature::Dust => "Dust belt",
            BeltFeature::DwarfPlanets => "Contains several large dwarf planets",
            BeltFeature::Wide => "Very wide, covering several orbits",
            BeltFeature::MineralRich => "Intensely mineral-rich asteroids",
        }
    }
}

/// Native life types (see `worldgen::life` for the rules).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifeType {
    Microbial,
    Simple,
    Ocean,
    Subsurface,
    Forests,
    Giant,
    Hostile,
    ColonyAltered,
    Extinct,
    SkyPlankton,
    SkyEcosystem,
    SkyLeviathans,
}

impl LifeType {
    pub fn name(&self) -> &'static str {
        match self {
            LifeType::Microbial => "Microbial Only",
            LifeType::Simple => "Simple Life",
            LifeType::Ocean => "Ocean Life",
            LifeType::Subsurface => "Subsurface Life",
            LifeType::Forests => "Forests and Wildlife",
            LifeType::Giant => "Giant Wildlife",
            LifeType::Hostile => "Hostile Wildlife",
            LifeType::ColonyAltered => "Colony-Altered Ecosystem",
            LifeType::Extinct => "Extinct Life",
            LifeType::SkyPlankton => "Sky Plankton",
            LifeType::SkyEcosystem => "Sky Ecosystem",
            LifeType::SkyLeviathans => "Sky Leviathans",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            LifeType::Microbial => "Microscopic life only: slimes, mats and films on rock and in water. Nothing visible beyond stains and scum.",
            LifeType::Simple => "Mosses, lichens, fungus-like growths and small soft-bodied creatures. No forests and no large animals.",
            LifeType::Ocean => "Life is confined to the seas, which may teem with creatures, while the land stays barren.",
            LifeType::Subsurface => "Life survives out of sight: in caves, deep rock, or an ocean beneath the ice. The surface looks dead.",
            LifeType::Forests => "A full ecosystem: plant-like vegetation, grazers, hunters and scavengers across land and sea.",
            LifeType::Giant => "A full ecosystem dominated by enormous creatures. Unsafe to cross without heavy vehicles.",
            LifeType::Hostile => "A lethal ecosystem of aggressive hunters, parasites and venomous life. Anything that moves is prey.",
            LifeType::ColonyAltered => "Native life tangled with crops, livestock and microbes brought in by the colony. Invasive species on both sides.",
            LifeType::Extinct => "Life once flourished here and died out. Fossils, petrified forests and dead seabeds remain.",
            LifeType::SkyPlankton => "Tiny drifting organisms floating in the gas giant's cloud layers.",
            LifeType::SkyEcosystem => "Floating gasbags, gliders and swarms that spend their whole lives in the clouds.",
            LifeType::SkyLeviathans => "Vast floating creatures, some kilometres across, drifting among smaller sky life.",
        }
    }

    /// Big, obvious life on land (forests, herds).
    pub fn visible(&self) -> bool {
        matches!(self, LifeType::Forests | LifeType::Giant | LifeType::Hostile | LifeType::ColonyAltered)
    }

    pub fn aerial(&self) -> bool {
        matches!(self, LifeType::SkyPlankton | LifeType::SkyEcosystem | LifeType::SkyLeviathans)
    }

    /// % chance of a 1st / 2nd / 3rd sentient species evolving.
    pub fn sentient_odds(&self) -> [f64; 3] {
        match self {
            LifeType::Ocean => [4.0, 2.0, 0.0],
            LifeType::Subsurface => [2.0, 0.0, 0.0],
            LifeType::Forests => [10.0, 5.0, 1.0],
            LifeType::Giant => [12.0, 6.0, 2.0],
            LifeType::Hostile => [8.0, 4.0, 1.0],
            LifeType::ColonyAltered => [6.0, 2.0, 0.0],
            LifeType::SkyEcosystem => [8.0, 4.0, 1.0],
            LifeType::SkyLeviathans => [10.0, 5.0, 1.0],
            _ => [0.0, 0.0, 0.0],
        }
    }

    /// Living biomass the drones can harvest for carbon and nitrogen.
    pub fn biomass(&self) -> bool {
        matches!(
            self,
            LifeType::Forests | LifeType::Giant | LifeType::Hostile | LifeType::Ocean | LifeType::ColonyAltered
        )
    }

    /// Wildlife that attacks drones.
    pub fn dangerous(&self) -> bool {
        matches!(self, LifeType::Hostile | LifeType::Giant)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeTech {
    StoneAge,
    BronzeIron,
    Medieval,
    EarlyIndustrial,
    Industrial,
    Atomic,
    SpaceAge,
    Interplanetary,
    Interstellar,
    AdvancedInterstellar,
    PostApocStone,
    PostApocIndustrial,
    PostApocAtomic,
    PostApocSpace,
    ExtinctRuins,
}

impl NativeTech {
    pub fn name(&self) -> &'static str {
        match self {
            NativeTech::StoneAge => "Stone Age",
            NativeTech::BronzeIron => "Bronze/Iron Age",
            NativeTech::Medieval => "Medieval",
            NativeTech::EarlyIndustrial => "Early Industrial",
            NativeTech::Industrial => "Industrial",
            NativeTech::Atomic => "Atomic / Information Age",
            NativeTech::SpaceAge => "Space Age",
            NativeTech::Interplanetary => "Interplanetary",
            NativeTech::Interstellar => "Interstellar",
            NativeTech::AdvancedInterstellar => "Advanced Interstellar",
            NativeTech::PostApocStone => "Post-Apocalyptic Stone Age",
            NativeTech::PostApocIndustrial => "Post-Apocalyptic Industrial",
            NativeTech::PostApocAtomic => "Post-Apocalyptic Atomic Age",
            NativeTech::PostApocSpace => "Post-Apocalyptic Space Age",
            NativeTech::ExtinctRuins => "Extinct Ruins",
        }
    }

    /// Society stage driving the faction / power tables.
    pub fn tier(&self) -> &'static str {
        match self {
            NativeTech::StoneAge => "tribal",
            NativeTech::BronzeIron | NativeTech::Medieval => "early",
            NativeTech::EarlyIndustrial | NativeTech::Industrial | NativeTech::Atomic => "industrial",
            NativeTech::SpaceAge | NativeTech::Interplanetary | NativeTech::Interstellar | NativeTech::AdvancedInterstellar => "spacefaring",
            NativeTech::ExtinctRuins => "extinct",
            _ => "collapsed",
        }
    }
}

/// A native civilisation on the world where its species evolved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NativeSociety {
    /// Name the species is known by.
    pub species: String,
    /// Living sentient species that evolved here.
    pub species_count: u8,
    pub tech: NativeTech,
    pub population: f64,
    pub factions: String,
    pub dominant_power: String,
    pub in_orbit: String,
    /// How they get on with a colony on the same world (if any).
    pub relations: String,
}

/// Off-world settlers living on (or abandoned on) this body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColonyTech {
    FrontierOutpost,
    SurveyInfrastructure,
    Settled,
    Industrial,
    Advanced,
    Fallen,
    Derelict,
}

impl ColonyTech {
    pub fn name(&self) -> &'static str {
        match self {
            ColonyTech::FrontierOutpost => "Frontier Outpost",
            ColonyTech::SurveyInfrastructure => "Survey Infrastructure",
            ColonyTech::Settled => "Settled Colony",
            ColonyTech::Industrial => "Industrial Colony",
            ColonyTech::Advanced => "Advanced Colony",
            ColonyTech::Fallen => "Fallen Colony",
            ColonyTech::Derelict => "Derelict Colonial Ruins",
        }
    }

    pub fn active(&self) -> bool {
        !matches!(self, ColonyTech::Fallen | ColonyTech::Derelict)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Colony {
    pub name: String,
    /// The people who founded it.
    pub founders: String,
    pub size: String,
    pub tech: ColonyTech,
    pub population: f64,
    pub missions: String,
    pub factions: String,
    pub faction_type: String,
    pub in_orbit: String,
}

/// Recoverable wreckage: drones strip it for structural material and metal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Salvage {
    pub what: String,
    /// Kilograms of usable material left.
    pub remaining: f64,
    pub initial: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Atmo {
    pub pressure_bar: f64,
    pub n2: f64,
    pub o2: f64,
    pub co2: f64,
}

impl Atmo {
    pub fn describe(&self) -> String {
        let main = if self.co2 > self.n2 && self.co2 > self.o2 {
            "CO2"
        } else if self.o2 > self.n2 {
            "O2"
        } else {
            "N2"
        };
        let mut s = format!("{}-dominated, {:.2} bar", main, self.pressure_bar);
        if self.o2 > 0.05 {
            s.push_str(&format!(", {:.0}% O2", self.o2 * 100.0));
        }
        s
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deposit {
    pub res: Res,
    /// Remaining amount in metric tons.
    pub tons: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Planet {
    pub name: String,
    pub class: PlanetClass,
    pub mass: f64,   // Earth masses
    pub radius: f64, // Earth radii
    /// Distance from its host star (AU). Moons carry their parent's.
    pub au: f64,
    pub period: f64,  // years
    pub temp: f64,    // K
    pub gravity: f64, // g (Earth = 1)
    pub atmo: Option<Atmo>,
    pub bulk: String,
    /// Number of moons (gas giants: the length of `satellites`).
    pub moons: usize,
    pub deposits: Vec<Deposit>,
    /// Initial orbital phase so the map looks alive and is deterministic.
    pub phase: f64, // radians

    // ---- detail from the world generator ----
    pub host: Host,
    /// 1 inner, 2 middle (habitable band), 3 outer.
    pub zone: u8,
    pub is_moon: bool,
    pub size_km: f64,
    pub atmosphere: Option<AtmoKind>,
    pub temp_band: TempBand,
    pub geosphere: Option<Geosphere>,
    /// Distinctive terrain features.
    pub terrain: Vec<String>,
    /// The big picture of the world's geography.
    pub surface: String,
    pub gg_feature: Option<GgFeature>,
    pub belt_feature: Option<BeltFeature>,
    pub ring: bool,
    pub tidally_locked: bool,
    /// Detailed moons (gas giants), dockable as `Loc::Moon`.
    pub satellites: Vec<Planet>,
    pub life: Option<LifeType>,
    pub natives: Option<NativeSociety>,
    pub colony: Option<Colony>,
    pub salvage: Option<Salvage>,
    /// Sentient species from elsewhere living here.
    pub settlers: Vec<String>,
}

impl Planet {
    pub fn exhausted(&self) -> bool {
        self.deposits.iter().all(|d| d.tons < 1.0)
    }

    /// Can the drones work anything here (ores, ring ice or salvage)?
    pub fn mineable(&self) -> bool {
        self.class.mineable() || self.ring
    }

    pub fn scoopable(&self) -> bool {
        self.class.scoopable()
    }

    /// Radiation from the body itself (a giant's belts), 0..1.
    pub fn radiation(&self) -> f64 {
        if self.gg_feature == Some(GgFeature::IntenseRadiation) { 0.5 } else { 0.0 }
    }

    /// Hull damage per hour from the air while docked (before shields).
    pub fn corrosion(&self) -> f64 {
        match self.atmosphere {
            Some(AtmoKind::Corrosive) => 0.6,
            Some(AtmoKind::Infiltrating) => 1.2,
            _ => 0.0,
        }
    }

    /// Crew can stretch their legs outside.
    pub fn shore_leave(&self) -> bool {
        self.atmosphere == Some(AtmoKind::Breathable)
            && matches!(self.temp_band, TempBand::Cold | TempBand::Temperate)
            && !self.life.is_some_and(|l| l.dangerous())
    }

    /// One-line kind description for lists ("Ice moon", "Gas giant", …).
    pub fn kind_label(&self) -> String {
        if self.is_moon {
            if self.class == PlanetClass::IceWorld { "Ice moon".into() } else { "Moon".into() }
        } else {
            self.class.name().into()
        }
    }
}

/// "<star> <roman numeral>" for the index-th world (0 = innermost).
pub fn planet_name(star_name: &str, index: usize) -> String {
    format!("{} {}", star_name, roman(index + 1))
}

pub fn roman(n: usize) -> String {
    const MAP: [(usize, &str); 5] = [(10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I")];
    let mut out = String::new();
    let mut n = n;
    for (v, s) in MAP {
        while n >= v {
            out.push_str(s);
            n -= v;
        }
    }
    out
}
