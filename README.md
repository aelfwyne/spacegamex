# FTL Drift

A windowed spaceship survival sim. Rust + [egui/eframe](https://github.com/emilk/egui).
One ship, one universe, no respawn: the galaxy is infinite, procedurally
generated from a seed, and does not care whether you live.

The game is a native desktop window with painted viewports: a live orbit
view of the current system (shaded planets, the star, your ship moving
between them), a pannable/zoomable star map, and an FTL warp view during
jumps. Everything can be done with the mouse; every action also has a
keyboard shortcut.

## Install (Linux / KDE Plasma)

Requires a Rust toolchain (https://rustup.rs) and the usual desktop graphics
stack (Vulkan or OpenGL drivers, Wayland or X11; standard on any KDE system).

```
./install.sh                    # release build + install for this user
INSTALL_DIR=/other/path ./install.sh
./install.sh --uninstall        # remove binaries, launcher and icon
```

This installs:

| What | Where |
|------|-------|
| `spacegame` (the game) and `spacegame-sim` (headless simulator) | `~/.local/bin/` |
| Application-menu launcher "FTL Drift" | `~/.local/share/applications/spacegame.desktop` |
| Icon | `~/.local/share/icons/hicolor/256x256/apps/spacegame.png` |

After installing, launch **FTL Drift** from the KDE application menu (under
Games), or run `spacegame` from a terminal. No install needed to just try it:
`cargo run` works straight from the source tree.

## Run

```
cargo run                                   # the game (debug build)
cargo run --release -- --seed 42            # a specific galaxy
spacegame --seed 0x2a                       # installed; decimal or hex seeds
```

Headless balance simulator (used by the regression battery):

```
cargo run --release --bin spacegame-sim -- --sim 500 --ship 0|1|2 --seed N
                                            # 500 in-game hours, exit 1 = died
cargo test                                  # unit tests, balance battery, GUI smoke test
cargo test all_ships_survive_500h_seed42    # 3 ships x seed 42, 500 h, 0 losses, >=1 jump
```

`cargo test` includes a GUI smoke test that renders every screen (title,
cockpit, mid-shuttle, mid-flight, paused, map, every panel, game over) for
all three ships at several window sizes with no GPU, so a layout panic is
caught before a player sees it.

## The game

You start at the origin system in one of three ships:

| # | Ship | Crew | What it is |
|---|------|------|------------|
| 1 | Keelclipper 'Meridian' | 1 | Light, efficient, forgiving — everything runs on one person's judgement. |
| 2 | Cargomaster 'Providence' | 5 | Real industrial capacity: mining, fab and fuel on a proper scale. |
| 3 | Ark 'Hesperide' | 10 + 500 pax | Enormous industrial plant — and an enormous mouth to feed. The hardest way to survive. |

The objective is simply **to survive**, indefinitely. The ship has:

- **Fuel (H2)** — scooped from stars and giant atmospheres. Never made on
  board. Scoop at the star, or shuttle to a gas/ice giant.
- **Water / Ice** — ice mines into water; the on-board fab converts it and
  splits it into O2. The single most important resource: it feeds drinking,
  the oxygen fab and the food fab.
- **Carbon / Nitrogen** — fab feedstock for food (and medicine). Rarer than
  ice; on cold worlds and in the outer systems.
- **Metal / Silicate** — hull material, fab input for repairs and medicine,
  and the only bulk cargo worth selling at a resupply station.
- **O2 / Food / Meds / Hull material** — made on board from the raws by the
  nanobot fab. If the raws stop, the tanks only go one way.

### Systems are real places

Every system is generated in detail (the generator is adapted from a
tabletop star-system generator the author wrote earlier):

- **Stars** with a spectral and luminosity class: mostly red dwarfs, as
  in the real galaxy, plus orange, yellow and white main-sequence stars,
  subgiants, giants and white dwarfs. Each has a real mass, radius,
  luminosity, temperature and habitable zone.
- **Binaries** (about 1 in 20): a *close* pair that planets orbit together
  (circumbinary), or an *intermediate*, *wide* or *distant* companion that
  strips away unstable orbits and may have planets of its own. Stability
  limits follow Holman & Wiegert (1999). A wide companion is a second star
  you can shuttle to and scoop.
- **Gas giants** with features (storms, a single super-storm, rings, high
  winds, intense radiation fields, or small ice giants) and **5–10
  moons each**, every one a full world you can dock at.
- **Terrestrial and ice worlds** with size, gravity, atmosphere (thin,
  breathable, toxic, dense, corrosive, infiltrating), temperature band,
  geosphere (desert → water world), a surface layout, and distinctive
  terrain features. Close-in worlds around red dwarfs may be tidally
  locked.
- **Asteroid belts**: rich in metal and silicate; some are mineral-rich,
  wide, dusty or studded with dwarf planets.
- **Native life** — microbial mats to forests, giant or hostile wildlife,
  ocean life, subsurface life, sky plankton and floating leviathans, or
  the fossils of extinct life.
- **Sentient natives** (Stone Age to Advanced Interstellar, collapsed
  post-apocalyptic societies, or only ruins), with population, politics and
  orbital infrastructure; spacefaring ones settle their neighbouring worlds.
- **Colonies** planted by other peoples: frontier outposts, survey
  stations, settled, industrial and advanced colonies — and fallen or
  derelict ones.

Every world has **finite tonnage deposits** that follow from what it is
(volume-scaled, so a small moon is a pocket, not a mine). Distant ore
worlds are not free: the shuttle round trip is real time, and the fab keeps
drinking while the drones sit in transit. A base test (mine rate vs. fab
draw, duty-weighted, per ore and in total) decides whether a system can
actually sustain a ship — most cannot (about one system in six), and the
autopilot knows it.

**Trade** comes from who actually lives there. Space-age natives or a
working colony run a T2 market (limited food, water, medicine and
materials — a waypoint, not a life). Interstellar natives or an
industrial colony run a T3 station that stocks two million tons of
everything: the only true resupply. Pre-space natives can only be
observed. About 2 systems in 100 hold any civilisation at all; under 1 in
100 will trade.

### What worlds do to you

| Trait | Effect while docked |
|---|---|
| Corrosive / infiltrating air | Hull damage (halved with shields); infiltrating air also causes life-support faults |
| Breathable air, cold or temperate, no dangerous wildlife | Shore leave: crew morale rises |
| A giant's intense radiation fields (and its moons) | Radiation damage unless shields are up |
| Storms, high winds, a super-storm | Scoop yield ×0.95 / ×0.85 / ×0.7 |
| Sky ecosystems | Scooping also strains out carbon |
| Hostile or giant wildlife | Drone faults three times as likely while mining |
| Living biospheres | Extra carbon and nitrogen deposits |
| Asteroid belts | More micrometeorite strikes (dust belts most of all) |
| Close binaries, white dwarfs | Their own radiation levels; a close pair is scooped together |
| Derelict colonies, ruins, abandoned stations | **Salvage** (`V`): drones strip structural material and metal, sometimes valuables |

### FTL

Nearest-neighbor jumps run on a precharge-then-burn model: a neighbor ~10 ly
away takes precharge plus flight — tens of in-game hours at 1x, so a jump is
a commitment, not a click. During flight the screen swaps to the trip
viewer, the fab keeps running off the raw tanks, and nothing can be
scooped. The autopilot (and the jump gate) will not commit to a crossing the
life support cannot outlast.

### Time

1 real second = 1 game second by default. `F` (or the buttons in the top
bar) cycles time compression 1x / 10x / 60x / 600x / pause; during a flight
`X` skips to arrival. The sim step
is continuous, so fast-forward is exact, not a shortcut.

### How the ship is flown

In the game you fly it: where to shuttle, where to jump, what to mine and
what to trade are all your commands (see the keys below). The plant runs
itself once toggled — FTL capacitors charge over time, the fab processes
according to its focus setting, and life support, scoop and drones obey
their switches — but nothing about *where* or *when* is autonomous.

There is also a hidden autopilot: the decision logic that makes a ship
survive indefinitely, picking destinations, shuttling to ore worlds,
splitting the drones between feedstock and buffers, and refusing crossings
it cannot outlast. It does not fly your ship in interactive play; it exists
to make the balance honest, driving the headless simulator (`spacegame-sim`) and
the regression tests to prove the game is survivable.

## Controls

### Mouse

- **Cockpit (system view):** click a world or moon to select it;
  double-click a world, moon, belt or star to shuttle there. Drag to pan,
  scroll to zoom; zoom in or select a gas giant to see its moons. In a
  binary, the view buttons frame star A, star B or both. Hover for details. The buttons under
  the view step through planets, shuttle, or open the planet page.
- **Star map:** drag to pan, scroll (or pinch) to zoom, click a star to
  select it, double-click to set it as the jump destination. The side panel
  lists every system within 30 ly with flight-time and fuel estimates, and
  the **JUMP** button.
- **Top bar:** time compression, save, help, quit, and tabs for every screen
  (Cockpit, Star map, Planets, Star, Ship systems, Resources, Crew, Trade).
- **Right column (cockpit):** reactor, scoop, drones, shields, lights,
  repair, fab focus, FTL throttle and the jump button. The log at the bottom
  can be resized by dragging its top edge.

### Keys

**Title:** `1`/`2`/`3` or arrows pick a ship, `Enter` starts, `C` continues
a saved game, `H` help, `Q` quit. (Typing in the seed box never fires
shortcuts.)

**In game:**

| Key | Action |
|-----|--------|
| `1` | Toggle fuel scoop |
| `2` | Toggle mining drones |
| `3` | Cycle fab focus (life support / repair / medical / stockpile / idle) |
| `4` | Cycle reactor output |
| `5` | Cycle FTL throttle |
| `6` | Ship systems |
| `S` | Shuttle star ↔ selected planet |
| `←` / `→` | Select previous / next planet (cockpit, planet page) |
| `P` | Next planet / planet page |
| `I` | Star page |
| `C` | Crew |
| `R` | Resources |
| `M` | Star map (jump destinations) |
| `K` | Trade (at T2/T3 systems) |
| `T` / `Enter` (cockpit) | Jump, once a destination is set and charged |
| `F` | Time compression 1x→10x→60x→600x→paused |
| `X` | Skip to arrival (in flight; works while paused) |
| `V` | Salvage on/off (drones strip wreckage instead of mining) |
| `G` | Save (autosaves hourly too) |
| `H` | Help |
| `Q` / `Ctrl+C` / closing the window | Save and quit |
| `Esc` | Back |

Map: `↑`/`↓` or `1`–`9` select, `Enter` sets/clears the destination, `T`
jumps, `C` clears. Trade: `↑`/`↓` select, `B` buy, `S` sell, `+`/`-`
quantity ×10, `Y` shipyard hull repair (T3).

## The universe

Seeded and deterministic: the same seed generates the same galaxy, in
detail, forever. Space is an infinite 10-light-year grid; every cell holds
0–3 systems, scattered anywhere inside the cell, and systems are generated lazily on first visit, so the galaxy
has no edge. Home is always the origin cell. Positions are pure functions
of system IDs, so a ship can never get "lost" — only unlucky.

Seeds worth trying: `42` (the regression battery seed — all three ships
must survive 500 h), `7`, `100`, `2026`, `1234`, `999`. The home system is
always a yellow G star with a gas giant, an ice giant and workable rocky
inner worlds, and never has a civilisation or a companion.

### Galaxy options

Under the title-screen buttons, three options shape a new galaxy. With the
seed they fully determine it: the same seed keeps the same star map, while
the options change what the systems hold.

- **Resources** — Scarce / *Normal* / Abundant: how common and how deep
  deposits are, and how fast drones and scoops work.
- **Civilization** — *Scarce* / Normal / Abundant: how many natives and
  colonies there are, and how many of them are spacefaring and trade.
- **Risk** — Tourist / *Normal* / Risky: how often breakdowns, hazards and
  micrometeorite impacts happen.

Defaults are in italics. The options are stored in the save.

## Balance notes

The balance target: **under autopilot, every ship survives 500 h with zero
crew or passenger losses and at least one jump, on every seed tried** —
and dies catastrophically if the wrong resource runs out first (which still
happens on bad runs: a colony with an empty nitrogen tank does not get a
warning, it gets a countdown).

The hard-won invariants (each one exists because a specific seed taught the
game a lesson):

- **A base must actually be able to sustain the ship.** Mine rate is judged
  duty-weighted (shuttle time is real) per ore and in total; a system whose
  ice sits at 40 AU is a monument, not a base.
- **Never chase what you can already work.** A critical ore that exists only
  at a market is the trade logic's problem; the drones commit only to real
  deposits, and only to ore this system can actually mine.
- **Hop only what you can outlast.** A destination the water cannot outlive
  is dropped, and precharging stops rather than standing for a crossing
  that will kill the crew.
- **Low water sends the ship home — but only from ground it cannot make
  good by staying.** The water-return mode is a hysteresis flag (enters well
  below the fab's refill equilibrium, leaves only once the water is back
  above it), so it cannot flap between "explore" and "return" at a healthy
  base.
- **Economy runs fab-first.** Raw feedstock (ice, carbon, nitrogen) costs
  0.5–1.2 credits a kilo at a T3 station and makes vitals that cost 3–12 to
  buy processed: the ship buys feedstock, fabs everything, and sells only
  the bulk ores the fab barely touches.

The regression battery (`cargo test all_ships_survive_500h_seed42`) plus a
manual sweep with `spacegame-sim` (seeds 1, 2, 3, 5, 7, 8, 11, 55, 77, 100, 123, 4242, 999,
1234, 2026, 31337, 4242, 99999 for the colony; 42/7/1234/999 for all
ships) currently passes 100%: zero losses across all 26 runs.

## Save / load

JSON in the per-user data directory:

- Linux: `$XDG_DATA_HOME/spacegame/save.json` (defaults to
  `~/.local/share/spacegame/save.json`)
- Windows: `%APPDATA%\spacegame\data\save.json`

Only systems you have visited are stored (everything else regenerates
identically from the seed), so saves stay small.

Autosaves every in-game hour and on quit (including closing the window);
`G` saves on demand, **Continue** on the title loads. Saves are written to a
temporary file and renamed, so a crash mid-save cannot corrupt the previous
save; if a save cannot be loaded, the title screen says why.

Save format 3 (since v0.3.0) uses the detailed world generator, so every
galaxy changed shape: saves from v0.2.0 and earlier cannot be continued (the
title screen says so) and start a new game instead. v0.3.0 saves load in
v0.4.0 with the default galaxy options.

## Building on Windows (future)

Nothing in the code is Linux-specific; the Windows path is meant to be a
build step, not a port:

- `cargo build --release` on a Windows machine with the MSVC Rust toolchain
  (`rustup default stable-x86_64-pc-windows-msvc`) produces
  `target\release\spacegame.exe` and `spacegame-sim.exe`. eframe uses
  Direct3D 12 / Vulkan there via wgpu; no extra SDKs are needed.
- The game binary is built as a GUI-subsystem app in release mode (no
  console window); `spacegame-sim` stays a console tool.
- Saves go to `%APPDATA%` automatically (the `directories` crate).
- Still to add when shipping on Windows: an `.exe` icon resource (e.g. via
  the `winres` crate in a `build.rs` gated on `cfg(windows)`), and an
  installer or `install.ps1` that creates a Start-menu shortcut. The window
  icon already works on every platform (it is embedded from `assets/icon.png`).

## Layout

```
src/lib.rs            crate root
src/bin/spacegame.rs      windowed game entry (eframe; app id "spacegame")
src/bin/spacegame-sim.rs  headless balance simulator (CLI)
src/game.rs           toolkit-agnostic game shell: screens, Action enum,
                      key bindings, real-time tick, save/load wiring
src/headless.rs       autopilot balance runner
src/rngx.rs           seeded derivation + procedural name generation
src/sim/
  galaxy.rs           infinite 10-ly grid, IDs, lazy system generation
  star.rs             spectral and luminosity classes, physics, scoop, radiation
  planet.rs           bodies and their traits (atmosphere, geosphere, terrain,
                      life, natives, colony, salvage, deposits)
  system.rs           a star system: stars, companion, bodies, sites, distances
  civ.rs              civilisation tiers (derived from natives/colonies), prices
  worldgen/           the detailed system generator:
    stars.rs            star roll, binary companions, Holman–Wiegert limits
    bodies.rs           gas giants + moons, terrestrial/ice worlds, belts
    terrain.rs          terrain catalog and surface layouts
    life.rs             native life, sentients, native societies
    colony.rs           colonies
    mod.rs              pipeline: orbits, naming, deposits, civ, home system
  res.rs              resources + inventory (+ neutral Rgb colors)
  ship.rs             the three ship specs
  state.rs            the sim: advance(), autopilot, mining, fab, trade,
                      FTL, life support, balance tests
  save.rs             save/load (atomic writes, per-OS data directory)
src/gui/
  mod.rs              eframe app, frame loop, shared top bar / log / toast
  theme.rs            colors, fonts, small widgets
  paint.rs            procedural drawing: shaded planets, stars, glows,
                      starfields
  sysview.rs          in-system orbit viewport
  tripview.rs         FTL warp viewport
  mapview.rs          star map + neighbor list
  panels.rs           cockpit controls, planet/star/resources/crew/
                      systems/trade/help pages
  title.rs            title / ship select, game over
assets/               icon.png, spacegame.desktop.in
```

The GUI never mutates game state directly: draw functions read the state
and return `Action`s, which `game::App::apply` executes. Porting to another
toolkit means rewriting `src/gui/` only.

### Development

Debug builds accept `SPACEGAME_PREVIEW=<screen>,<ship>,<hours>` to skip
straight to a screen with the autopilot having flown for a while, e.g.
`SPACEGAME_PREVIEW=map,2,40 cargo run -- --seed 7`. Screens: `cockpit`,
`trip`, `map`, `planet`, `star`, `systems`, `resources`, `crew`, `trade`,
`help`, `title`. An optional fourth field jumps to the nearest system with a
feature and selects it: `pbin`, `sbin`, `natives`, `colony`, `belt`,
`rings`, `storm`, `life`, `locked`, `corrosive`, `ice` (e.g.
`SPACEGAME_PREVIEW=planet,1,1,natives`). Release builds ignore it.
