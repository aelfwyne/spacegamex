# Changelog

## 0.8.0 — 2026-11-14

### Fixes
- **Space dock purchases go to the ship's hold** by default. If the ship is
  not docked at that colony, the purchase is refused with a clear message
  (nothing is charged); goods go straight into the colony only when you
  choose *Colony*. They used to be silently redirected to the colony.
- A new colony's supply, power and shelter readings are up to date at once
  (they read 0% until its first month had passed).

### Natives, hybrids and Genetic Modification Kits
- Each colonist is **Human, Native or Hybrid**. A **Genetic Modification
  Kit** licenses one human and native couple to have children together;
  their children are Hybrids, free colonists who have children only with
  other Hybrids. It no longer switches anything on colony-wide.
- Kits are stocked by the colony (unloaded or bought at the dock) and used
  as soon as a couple can be made: married human and native couples first
  (the captain's own first), then single natives married to humans. The
  Supplies tab lists the kits in store and the licensed couples.
- A captain's native partner is recorded as a native; only their own
  descendants are Hybrids.
- Partners can be sought among **pre-space natives** (about 1 in 10, no
  port needed). Partners from colonies are always human.
- The Census tab shows natives and hybrids, with captives (only they weigh
  on the colony) on the natives' line.

### Hegemony
- Hegemony is offered at the capital only.
- **Repealing Hegemony** (with a warning and a second click to confirm)
  frees every other colony; past a million people the capital sends the
  captain away too, and they fly on as a roaming captain.

## 0.7.0 — 2026-11-13

### Ships and the fleet
- **Shipyards sell ships:** every interstellar (T3) port sells the Company
  ship, and each stocks its own selection of six new hulls: the
  Interceptor *Kestrel* and Gunship *Sentinel*, the Heavy Frigate *Aegis*,
  the Fast Packet *Swallow*, and the Bulk Freighter *Ox* and
  Super-Freighter *Atlas*.
- **Trade in** the old ship against the price (40% of list, by condition),
  or pay in full and have the new one ferried to your colony's **fleet**.
- Switch between laid-up ships when docked at the colony. Each ship keeps
  its own stores, cargo, hull, crew and fittings; only the captain, their
  partner and their pilot change ships. Break up a laid-up ship for the
  colony's materials.

### Colony
- **Shipyard** (space dock, Business Center and 250,000 colonists): builds
  the three founding designs from the stockpile, and any other design once
  you buy its blueprints (half its price). Three slips.
- An **ark** built at the shipyard takes 500 of the colony's people as
  settlers, with a share of the royal line if it lives there (never your
  heirs). The census remembers who left, and when.
- **Hegemony** law: found more colonies, buy colony arks at any T3 yard,
  and at a million continue the Royal Lineage. Costs a little happiness and
  wealth.
- **Space dock:** purchases can go into the docked ship's hold instead of
  the colony, to fill an ark or carry supplies elsewhere. The dock restocks
  over the weeks like any port (it used to reroll once a year, so a bought-
  out dock stayed empty). Each manufacturing plant keeps supplies for 1,000
  more colonists on sale there, made from the stockpile and sold for less.
- Quest deliveries, after Attack and Defense, now award drone refits you
  can't buy: ship drone yield and colony Resource Drones capacity, +10% a
  time up to +50% each.

### Star map
- **Visited** tab: every system you have been to, at any range.
- **Target** a system (◎ in the list, double-click, or the *Target* button):
  a green edge arrow, crosshair marker and side-panel line point the way,
  like the colony arrow. The target is saved with the game.

### The fab
- **Fab focus works.** Every focus first makes what is being consumed.
  Then *Life support* fills water, O2 and food in full, *Repair* fills MAT,
  *Medical* fills medicine, and *Stockpile* tops every tank up evenly from
  the mining surplus. (Before, every focus but Idle did the same thing.)
- Water is made first, for what the oxygen, food and medicine lines will
  actually draw. The food refill used to drink the water tank dry while
  ice piled up unused.

### Fixes
- The ark's reactor is 800 MW (was 700): it could not power its own
  in-system load. Fuel per MW is unchanged.
- Autopilot: with an ore critical, the star and the planet used different
  fuel lines, so an ark at 70% H2 shuttled between them every 0.4 h, never
  mining or refuelling, until the crew died.

### Windows
- First Windows build: a zip with `colonia-galactica.exe` (icon embedded,
  no console window) and the simulator. Unzip and run; saves go to
  `%APPDATA%\colonia-galactica`. Built from Linux with `./build-windows.sh`.

0.6.0 saves load.

## 0.6.0 — 2026-11-10

### Captains
- **New game asks who you are:** given name, surname, sex and one of eight
  backgrounds (Naval Academy, Merchant Family, Asteroid Miner, Ship's
  Engineer, Field Medic, Smuggler, Colonial Surveyor, Noble Heir). Each adds
  starting credits and two lasting bonuses.
- Captains start at 22–25, straight out of flight school; a recruited
  partner is near the captain's age.
- The captain's surname is the family name of their line in the colony.

### Colony
- **Laws** (Colony › Laws): ten laws, each with pros and cons. Enacting
  costs materials, a law stands at least a year, repealing is free.
- **Family size depends on colony size:** a handful of settlers have big
  frontier families; a larger colony needs health, contentment, food and
  wealth to keep growing. A couple on a kind world now usually takes root.
- **Living off the land:** without hydroponics, colonists on habitable and
  marginal worlds forage at once and farm after a few years. It feeds them
  but is hard, slow labour.
- Softer inbreeding health penalty (it mainly raises infant deaths);
  parents and children never pair.
- Colony time slowed to about 96 seconds a year at 1x (was 24).

### Statistics, retirement and the scoreboard
- **Statistics screen** (`L`): counters for nearly everything, records
  (hottest and coldest worlds, gravity, size, moons, stars) and every kind
  of star, world, atmosphere, life and civilisation found.
- **Retire** at any time (Crew screen or colony Overview; asks twice).
- Winning, losing and retiring show a **score** breakdown and add it to the
  **Scoreboard** on the title screen (`S`), kept in `scores.json`.

### Star map
- An orange arrow at the chart's edge points to your colony when it is off
  the chart; the side panel shows its distance with *Show*, *Select* and
  *Best hop*. Each selected system shows its distance from the colony.

### Randomness
- New random number generator throughout (xoshiro256++ keyed through the
  moremur mixer, replacing ChaCha12 keyed through splitmix64). Babies' sex
  has its own stream. Tests check fairness, independence and sibling sex
  correlation.
- **Every galaxy is different from 0.5.0.** Old saves load, but the galaxy
  around them regenerates differently: start a new game.

### Fixes
- The home system no longer gives lifeless worlds living terrain.

## 0.5.0 — 2026-11-09

The colony update. The game is renamed **Colonia Galactica** (from FTL
Drift).

- Cargo hold and markets for colony supplies and goods; Year/Day clock;
  captain identity; recruiting a partner or pilot at ports.
- Pirates: rare auto-resolved battles, flee or pay off, upgrades and
  bounties.
- Colonies: founding, supplies, census of every colonist, buildings,
  retirement and succession, a million people to win.
- Colony screens: turning world, stats, supplies, building, census and
  family tree.
- Quests: relics, artifacts and port jobs.

## 0.4.0 — 2026-11-09

- Galaxy options on the title screen: resources, civilisation, risk.
- Repairs pay only for real damage and run in batches.
- Fuel and mining rebalanced; live burn and fuel net shown.
- Build stamp in the UI; smoothed resource rates.

## 0.3.0 — 2026-11-09

- Detailed star-system generation: moons, binary stars, life, natives and
  colonies. New save format; older saves cannot be continued.

## 0.2.0 and earlier

- FTL Drift: the windowed egui game, with simulation fixes.
