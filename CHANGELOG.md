# Changelog

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
