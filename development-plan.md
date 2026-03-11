# HYRR Development Plan

**Hierarchical Yield and Radionuclide Rates**

A pure Python package for predicting radio-isotope production in stacked target assemblies, replacing the Fortran-based ISOTOPIA with better stopping powers, compound/multilayer support, and depth-resolved output.

---

## 1. Vision

Replace the current architecture (Python wrapper → subprocess → Fortran binary + 2.1 GB data directory of 547,000 files) with a self-contained Python package backed by a single SQLite database. The result is `pip install hyrr` — no Fortran compiler, no container, no multi-GB download.

### What HYRR computes

1. **Stopping power** of any ion (p, d, t, ³He, α) in any material — via PSTAR/ASTAR table lookup with velocity scaling for d/t/³He (replacing ISOTOPIA's bare Bethe-Bloch)
2. **Energy-integrated production rates** — ∫ σ(E) / (dE/dx) dE across a target layer
3. **Time-dependent yield & activity** — Bateman equations for production, decay, and daughter in-growth during irradiation and cooling
4. **Depth profiles** — heat deposition and activity density as a function of depth in each layer
5. **Compound materials** — Bragg additivity for stopping power, isotope-resolved reaction channels
6. **Stacked layer geometries** — beam propagates through ordered layers (windows, targets, degraders, backings, monitor foils)

### Key design decisions

| Decision | Choice | Rationale |
|---|---|---|
| Language | Python (NumPy/SciPy) | Computation is trivial (~20 ms), entire ecosystem is Python |
| Data storage | SQLite (stdlib) | Indexed lookups by composite key, zero dependencies, single file |
| DataFrames | NumPy core, polars/pandas optional | Core computation uses plain numpy arrays — one code path for pip and WASM. Polars and pandas are lazy-imported only for export (`to_polars()`, `to_pandas()`), not required at runtime. |
| Materials | py-mat (MorePET/py-mat) | Already provides hierarchical materials with density + composition |
| Stopping powers | PSTAR/ASTAR tables | NIST reference data, replaces inaccurate bare Bethe-Bloch |
| Parallelism | Not needed | Single simulation < 20 ms; batch parallelism can be added later |

---

## 2. Architecture

```
hyrr/
├── data/
│   ├── build_db.py              # one-shot script: parse TENDL/PSTAR/ASTAR/decay → SQLite
│   └── hyrr.sqlite              # single file: cross-sections, stopping powers, abundances, decay
├── src/hyrr/
│   ├── __init__.py
│   ├── db.py                    # SQLite access layer (queries, interpolation helpers)
│   ├── stopping.py              # table lookup + Bragg additivity + d/t/³He velocity scaling
│   ├── production.py            # ∫σ/dEdx integration + Bateman equations + depth profiles
│   ├── models.py                # dataclasses: Beam, Layer, TargetStack, DepthPoint, Result
│   ├── materials.py             # bridge to py-mat + isotopic composition resolution
│   ├── plotting.py              # energy scans, depth profiles, cooling curves (matplotlib/plotly)
│   └── cli.py                   # optional CLI entry point
├── tests/
├── notebooks/
├── pyproject.toml
└── development-plan.md          # this file
```

### SQLite schema

```sql
-- TENDL/IAEA residual production cross-sections
-- Source: isotopia.libs/{p,d,t,h,a}/{Target}/iaea.2024/tables/residual/*
CREATE TABLE cross_sections (
    projectile  TEXT    NOT NULL,  -- 'p','d','t','h','a'
    target_Z    INTEGER NOT NULL,  -- target charge number
    target_A    INTEGER NOT NULL,  -- target mass number
    residual_Z  INTEGER NOT NULL,  -- product charge number
    residual_A  INTEGER NOT NULL,  -- product mass number
    state       TEXT    NOT NULL DEFAULT '',  -- '', 'g', 'm' (ground/metastable)
    energy_MeV  REAL    NOT NULL,
    xs_mb       REAL    NOT NULL,  -- cross-section in millibarns
    source      TEXT    NOT NULL DEFAULT 'iaea.2024',
    PRIMARY KEY (projectile, target_Z, target_A, residual_Z, residual_A, state, energy_MeV, source)
);

CREATE INDEX idx_reaction ON cross_sections(
    projectile, target_Z, target_A
);

CREATE INDEX idx_product ON cross_sections(
    residual_Z, residual_A, state
);

-- NIST PSTAR/ASTAR stopping power tables
-- Source: libdEdx data files (PSTAR.dat, ASTAR.dat, pstarEng.dat, astarEng.dat)
CREATE TABLE stopping_power (
    source      TEXT    NOT NULL,  -- 'PSTAR', 'ASTAR'
    target_Z    INTEGER NOT NULL,
    energy_MeV  REAL    NOT NULL,
    dedx        REAL    NOT NULL,  -- mass stopping power [MeV·cm²/g]
    PRIMARY KEY (source, target_Z, energy_MeV)
);

-- Natural isotopic abundances (IUPAC/NUBASE)
CREATE TABLE natural_abundances (
    Z            INTEGER NOT NULL,
    A            INTEGER NOT NULL,
    symbol       TEXT    NOT NULL,
    abundance    REAL    NOT NULL,  -- fractional (e.g., 0.0982 for Mo-100)
    atomic_mass  REAL    NOT NULL,  -- atomic mass [u]
    PRIMARY KEY (Z, A)
);

-- Decay data for Bateman equations
-- Source: isotopia decay data files
CREATE TABLE decay_data (
    Z              INTEGER NOT NULL,
    A              INTEGER NOT NULL,
    state          TEXT    NOT NULL DEFAULT '',  -- '', 'g', 'm'
    half_life_s    REAL,             -- NULL for stable isotopes
    decay_mode     TEXT    NOT NULL,  -- 'beta-', 'beta+', 'EC', 'alpha', 'IT', 'stable'
    daughter_Z     INTEGER,
    daughter_A     INTEGER,
    daughter_state TEXT    DEFAULT '',
    branching      REAL    DEFAULT 1.0,
    PRIMARY KEY (Z, A, state, decay_mode, daughter_Z, daughter_A, daughter_state)
);

CREATE INDEX idx_parent ON decay_data(Z, A, state);
CREATE INDEX idx_daughter ON decay_data(daughter_Z, daughter_A, daughter_state);
```

Estimated sizes:
- cross_sections: ~20M rows → ~200-300 MB
- stopping_power: ~19K rows → negligible
- natural_abundances: ~3,400 rows → negligible
- decay_data: ~5,000 rows → negligible

---

## 3. Data model

### Core types

```python
@dataclass
class Beam:
    projectile: str          # 'p', 'd', 't', 'h', 'a'
    energy_MeV: float        # incident energy
    current_mA: float        # beam current

@dataclass
class Element:
    symbol: str
    Z: int
    isotopes: dict[int, float]  # A → fractional abundance

    @classmethod
    def natural(cls, symbol: str) -> Element:
        """Load natural abundances from SQLite."""

    @classmethod
    def enriched(cls, symbol: str, enrichment: dict[int, float]) -> Element:
        """User-defined isotopic composition. Must sum to ~1.0."""

@dataclass
class Layer:
    material: Material           # from py-mat (provides density + elemental composition)
    isotopes: dict[str, Element] | None = None  # override isotopics per element, None → natural

    # exactly one of these three must be set:
    thickness_cm: float | None = None
    areal_density_g_cm2: float | None = None
    energy_out_MeV: float | None = None

    # computed after stack validation:
    _energy_in: float = field(init=False)
    _energy_out: float = field(init=False)
    _thickness: float = field(init=False)
    _is_monitor: bool = False

@dataclass
class TargetStack:
    beam: Beam
    layers: list[Layer]

    def validate(self) -> None:
        """Propagate beam through layers, compute thicknesses, check beam doesn't stop."""

@dataclass
class DepthPoint:
    depth_cm: float
    energy_MeV: float
    dedx_MeV_cm: float
    heat_W_cm3: float
    production_rates: dict[str, float]  # isotope_name → local rate [s⁻¹·cm⁻¹]

@dataclass
class LayerResult:
    layer: Layer
    energy_in: float
    energy_out: float
    delta_E_MeV: float
    heat_kW: float
    depth_profile: list[DepthPoint]
    isotope_production: dict[str, IsotopeResult]

@dataclass
class IsotopeResult:
    name: str                    # e.g., "Tc-99m"
    Z: int
    A: int
    state: str
    half_life_s: float | None
    production_rate: float       # [s⁻¹]
    saturation_yield_Bq_uA: float
    time_s: np.ndarray           # time points [s] (irradiation + cooling)
    activity_Bq: np.ndarray      # activity at each time point [Bq]
    yield_Bq_per_mAh: np.ndarray # yield at each time point [Bq/mAh]

@dataclass
class StackResult:
    stack: TargetStack
    layers: list[LayerResult]
    irradiation_time_s: float
    cooling_time_s: float

    def purity_at(self, cooling_time_s: float, isotope: str) -> float:
        """Fraction of total activity from specified isotope at given cooling time."""

    def to_polars(self) -> "pl.DataFrame":
        """All results as Polars DataFrame. Requires: pip install hyrr[polars]"""

    def to_pandas(self) -> "pd.DataFrame":
        """All results as Pandas DataFrame. Requires: pip install hyrr[pandas]"""

    def to_excel(self, path: str) -> None:
        """Formatted export for logbook/regulatory use."""

    def to_json(self) -> str:
        """Structured JSON export (config + results), re-importable."""

    def to_csv_bundle(self, path: str) -> None:
        """Zip of CSVs (one per layer, one summary, one per depth profile)."""

    def summary(self) -> str:
        """Printable text summary (similar to ISOTOPIA output format)."""
```

### py-mat integration

py-mat provides `Material` with `.density`, `.composition` (element → mass fraction), and `.formula`. HYRR resolves isotopics:

```python
# hyrr/materials.py

def resolve_isotopics(
    material: Material,
    overrides: dict[str, Element] | None = None,
) -> list[tuple[Element, float]]:
    """
    Resolve a py-mat Material into (Element with isotopes, atom_fraction) pairs.

    1. Get elemental composition from material.composition or parse material.formula
    2. Convert mass fractions → atom fractions
    3. For each element, use natural abundances unless overridden
    """
```

What py-mat needs (contribute upstream):
- Elemental `composition` for alloys (Havar, SS316L, etc.) in the TOML data files
- Auto-derive composition from `formula` for compounds (periodictable already available)

---

## 4. Physics modules

### 4.1 stopping.py — Stopping power

**Input**: projectile type, target element Z, energy [MeV]
**Output**: mass stopping power [MeV·cm²/g]

```python
def elemental_dedx(projectile: str, target_Z: int, energy_MeV: float) -> float:
    """
    Look up stopping power from PSTAR/ASTAR tables with interpolation.

    - p: direct PSTAR lookup
    - α: direct ASTAR lookup
    - d, t: PSTAR at E/A (velocity scaling), same Z as proton
    - ³He: ASTAR at E × (4/3) (velocity scaling), same Z as alpha

    Interpolation: SciPy interp1d (cubic) on log-log scale.
    """

def compound_dedx(projectile: str, material: Material, energy_MeV: float) -> float:
    """
    Bragg additivity: S_compound = Σ w_i × S_i(E)
    where w_i = mass fraction of element i.
    """
```

Data source: PSTAR (133 energies × 74 elements) + ASTAR (122 energies × 74 elements) from libdEdx `.dat` files.

### 4.2 production.py — Production rates & Bateman equations

**Production rate integration** (per isotope per layer):

```
prate = (N_proj / V_target) × ∫[E_out → E_in] σ(E) / dEdx(E) dE × 1e-27
```

- 100-point numerical quadrature (same as ISOTOPIA)
- σ(E) from cross_sections table with interpolation
- dEdx(E) from stopping.py (compound stopping power for the layer material)
- At each integration point, store depth, energy, local heat, local production rate → depth profile

**Bateman equations** (time evolution):

Direct from target:
```
N(t) = N₀ × R_prod / (λ - R_total) × [exp(-R_total·t) - exp(-λ·t)]    (irradiation)
N(t) = N(T_irr) × exp(-λ·(t - T_irr))                                   (cooling)
```

Daughter from parent decay:
```
N_D(t) += N_P(T_irr) × λ_P / (λ_D - λ_P) × [exp(-λ_P·Δt) - exp(-λ_D·Δt)]  (cooling)
```

Same physics as ISOTOPIA's `prodyield.f90`, just in NumPy.

**Depth profile output**:

At each of the 100 integration points per layer, record:
- depth [cm] (cumulative from 1/dEdx integration)
- energy [MeV]
- dE/dx [MeV/cm] → heat deposition density
- heat [W/cm³] = I_beam × dEdx × unit conversion
- production rate per isotope [s⁻¹·cm⁻¹]

After Bateman: convert local production rates to local activity densities at specified times.

---

## 5. Implementation phases

### Phase 1: Database & foundation

**Goal**: Parse all source data into SQLite, validate against ISOTOPIA output.

Tasks:
- [ ] Create new repo/branch for hyrr
- [ ] Write `data/build_db.py` — TENDL cross-section parser
  - Parse YANDF-0.2 header format from `isotopia.libs/{p,d,t,h,a}/*/iaea.2024/tables/residual/*`
  - 5 projectiles × 672 targets × ~200 residual files each → ~20M rows
  - Validate: spot-check parsed values against original files
- [ ] Write `data/build_db.py` — stopping power ingestion
  - Parse PSTAR.dat + pstarEng.dat (133 × 74 = 9,842 rows)
  - Parse ASTAR.dat + astarEng.dat (122 × 74 = 9,028 rows)
  - Source: libdEdx repository (APTG/libdedx) data files
- [ ] Write `data/build_db.py` — natural abundances
  - Source: IUPAC 2021 or NUBASE (well-established, ~3,400 isotopes)
- [ ] Write `data/build_db.py` — decay data
  - Parse from ISOTOPIA's decay data files in `isotopia/` directory
  - Half-lives, decay modes, branching ratios, daughter products
- [ ] Write `src/hyrr/db.py` — query layer
  - `get_cross_sections(projectile, target_Z, target_A) → list of (residual_Z, residual_A, state, E[], σ[])`
  - `get_stopping_power(source, target_Z) → (E[], dEdx[])`
  - `get_natural_abundances(Z) → dict[A, abundance]`
  - `get_decay_data(Z, A, state) → DecayInfo`
  - Connection management, caching of frequently accessed tables
- [ ] Store data version metadata in SQLite (`CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT)`) — track TENDL version, build date, hyrr version, source hashes. Enables reproducibility: shared configs can reference the exact data version used.
- [ ] Validate: compare queried cross-sections against ISOTOPIA sample outputs

### Phase 2: Stopping power

**Goal**: Replace Bethe-Bloch with PSTAR/ASTAR table lookup, handle all projectile types.

Tasks:
- [ ] Implement `elemental_dedx()` — PSTAR/ASTAR lookup with log-log cubic interpolation
- [ ] Implement velocity scaling for d, t, ³He
  - d, t: same charge as proton → PSTAR at E/A_projectile (energy per nucleon)
  - ³He: same charge as alpha → ASTAR at E × (A_alpha / A_He3) = E × 4/3
- [ ] Implement `compound_dedx()` — Bragg additivity over elemental mass fractions
- [ ] Validate: compare against SRIM/PSTAR web calculator for known materials
- [ ] Validate: compare target thickness calculation against ISOTOPIA for p+Mo100 sample case

### Phase 3: Single-layer production (ISOTOPIA parity)

**Goal**: Reproduce ISOTOPIA's output for a single pure-element target.

Tasks:
- [ ] Implement `models.py` — Beam, Layer, TargetStack, result dataclasses
- [ ] Implement production rate integration (100-point quadrature)
  - Energy grid from E_out to E_in
  - Stopping power at each point
  - Cross-section interpolation at each point
  - Sum: prate = N_proj/V × Σ σ(E_i) × (1/dEdx_i) × dE × 1e-27
- [ ] Implement Bateman equations
  - Target depletion
  - Direct production + decay
  - Daughter in-growth from parent beta decay
  - Irradiation phase + cooling phase
- [ ] Implement time grid (irradiation + cooling)
- [ ] Implement unit conversions (Bq/kBq/MBq/GBq/Ci, yield units)
- [ ] **Validate against ISOTOPIA sample cases**:
  - p + Mo100 → Tc99m (16 → 12 MeV, 0.15 mA, 1d irradiation, 1d cooling)
  - Compare: production rates, activities, yields, T_max for all products
  - Accept ≤5% deviation from ISOTOPIA (due to improved stopping powers)
  - Document and explain any deviations

### Phase 4: Compound materials & py-mat integration

**Goal**: Support compound targets (MoO₃, enriched materials, alloys).

Tasks:
- [ ] Implement `materials.py` — bridge between py-mat Material and hyrr Element
  - Parse composition from py-mat material
  - Resolve natural isotopic abundances per element
  - Support enrichment overrides
  - Mass fraction → atom fraction conversion
- [ ] Contribute upstream to py-mat: add composition data for common alloys
  - Havar (Co 42.5%, Cr 20%, Ni 13%, W 2.8%, Mo 2%, Mn 1.6%, C 0.2%, Fe 17.9%)
  - SS316L, copper, aluminum, niobium, titanium, silver, etc.
- [ ] Compound stopping power via Bragg additivity
- [ ] Compound production: iterate over all isotopes of all elements in compound
  - Each isotope weighted by atom fraction × isotopic abundance
  - All reaction channels for each isotope
- [ ] Test: MoO₃ target (natural vs enriched Mo-100)
- [ ] Test: Havar window activation inventory

### Phase 5: Stacked layers

**Goal**: Beam propagation through multiple layers with per-layer results.

Tasks:
- [ ] Implement `TargetStack.validate()`
  - Walk beam through layers: E_in → compute E_out → next layer E_in = previous E_out
  - For each layer, compute thickness/areal_density/energy_out from whichever is specified
  - Error if beam stops in any layer (E_out ≤ 0)
  - Error if energy_out constraint is unreachable
- [ ] Implement per-layer production calculation
  - Each layer gets its own energy window [E_out, E_in]
  - Independent isotope production per layer
  - Shared Bateman time grid across all layers
- [ ] Implement `StackResult` aggregation
  - Per-layer: energy_in, energy_out, ΔE, heat, all isotope results
  - Total: combined activity inventory across all layers
- [ ] Test: Havar window + Mo-100 target + Cu backing
- [ ] Test: degrader foil + thin target + monitor foil

### Phase 6: Depth profiles

**Goal**: Spatially resolved heat and activity distributions.

Tasks:
- [ ] Store integration points during production calculation (depth, E, dEdx, rates)
  - Already computed in the quadrature loop — just retain instead of discarding
- [ ] Compute per-point quantities:
  - heat_W_cm3 = I_beam [A] × dEdx [MeV/cm] × 1.602e-13 [J/MeV] / area [cm²] ... per unit depth
  - activity_density per isotope at each depth point (after Bateman)
- [ ] `LayerResult.depth_profile` → list of DepthPoint or Polars DataFrame
- [ ] `StackResult` — concatenated depth profile across all layers with cumulative depth
- [ ] Plotting: heat vs depth, activity vs depth, with layer boundaries marked

### Phase 7: Analysis features

**Goal**: The tools researchers actually need for production planning.

Tasks:
- [ ] **Saturation yield**: Y_sat = λ × prate / I_beam [Bq/μA] — standard comparison metric
- [ ] **Energy scan**: sweep E_beam (or E_out), plot activity/yield/purity vs energy
  - Identify optimal energy window for desired product
  - Show contaminant ratios
- [ ] **Irradiation time optimization**: activity vs time curve with T_max marked
- [ ] **Cooling curves**: activity of all products from beam-off through user-defined time
- [ ] **Purity at time of use**: `result.purity_at(cooling_time, isotope)` → fraction of total activity
- [ ] **Monitor reactions**: flag layers as monitors, compare computed vs expected activity for beam current verification
- [ ] **Beam current scan**: activity and heat vs current, with thermal warnings
- [ ] **Full decay chains**: extend Bateman solver beyond single parent→daughter to full chains (e.g., Ac-225 → Fr-221 → At-217 → ...). Required for therapeutic isotopes where daughter activities matter clinically.
- [ ] **Uncertainty propagation**: run simulations with ±1σ on cross-sections (TENDL provides covariance data). Simple approach: repeat integration with perturbed σ values, report uncertainty bands on activity/yield.
- [ ] **General parameter sweep API**: `hyrr.sweep(stack, param="beam.energy_MeV", values=range(10, 30))` — sweep any parameter (energy, current, enrichment, thickness), return results as DataFrame. Subsumes energy scan and current scan as special cases.
- [ ] **CLI `hyrr compare`**: diff two results (from JSON files or two inline runs). Side-by-side table of activities, yields, deviations. Researchers constantly compare "what if I change energy from 16 to 18 MeV?"

### Phase 8: Output & reporting

**Goal**: Get results out of notebooks and into papers/reports.

Tasks:
- [ ] `result.to_polars()` — lazy-import polars, export all results as Polars DataFrame (`pip install hyrr[polars]`)
- [ ] `result.to_pandas()` — lazy-import pandas, export all results as Pandas DataFrame (`pip install hyrr[pandas]`)
- [ ] `result.summary()` — text output similar to ISOTOPIA format (for comparison/validation)
- [ ] `result.to_excel()` — formatted spreadsheet for logbook/regulatory use
- [ ] `result.to_json()` — structured JSON export (config + results), machine-readable, re-importable
- [ ] `result.to_csv_bundle(path)` — zip of CSVs (one per layer, one summary, one per depth profile)
- [ ] `StackResult.from_json(path_or_str)` — re-import a saved JSON result
- [ ] `TargetStack.to_config_json()` — export just the input config (beam + layers + times), lightweight
- [ ] `TargetStack.from_config_json()` — reconstruct a run from saved/shared config
- [ ] Plotting module (`plotting.py`):
  - Depth profiles (heat, activity) with layer annotations
  - Activity vs time (irradiation + cooling) per isotope
  - Energy scan results
  - Purity vs cooling time
  - Support both matplotlib (publication) and plotly (interactive/notebook)
  - EXFOR experimental data overlay on cross-section plots (measured vs TENDL curves) — low effort, high value for validation and publications
- [ ] `hyrr compare` output: formatted diff table (terminal + HTML) for two StackResults

### Phase 9: Interactive web frontend

**Goal**: A serverless web application on GitHub Pages — zero backend, zero cost, zero auth. Users configure beam/target parameters, run simulations in-browser, and explore results interactively.

#### 9a: Marimo prototype (quick win)

[Marimo](https://marimo.io) reactive notebooks as a local/demo app while the full frontend is developed.

Tasks:
- [ ] Add `marimo` to optional dependencies (`pip install hyrr[app]`)
- [ ] Build interactive notebook/app with beam config, target stack builder, live plots
- [ ] Deploy as standalone app via `marimo run` (containerized or local)
- [ ] Add preset configurations for common production scenarios (Tc-99m, F-18, etc.)

#### 9b: Serverless WASM frontend (primary target)

Fully client-side: Pyodide runs hyrr in the browser, nuclear data is lazy-loaded and cached, history lives in IndexedDB. Hosted as static files on GitHub Pages.

**Architecture:**

```
GitHub Pages (static files, free)
├── index.html + bundle.js (Svelte)
├── pyodide + hyrr + numpy + scipy (WASM, ~28 MB, cached)
├── data/
│   ├── schema.sql                   # CREATE TABLE + CREATE INDEX statements
│   ├── meta.sql.gz                  # INSERT statements: abundances + decay (~100 KB)
│   ├── stopping.sql.gz              # INSERT statements: PSTAR/ASTAR (~200 KB)
│   └── xs/
│       ├── p_Mo.sql.gz              # INSERT statements: p + Mo cross-sections (~500 KB each)
│       ├── p_O.sql.gz
│       ├── a_Bi.sql.gz
│       └── ...                      # ~370 files (5 projectiles × 74 elements)

Browser
├── Compute:   Pyodide + hyrr + sql.js (SQLite in WASM)
├── Plots:     Plotly.js
├── History:   IndexedDB (full run configs + results, persistent, unlimited)
├── Settings:  localStorage
└── Sharing:   URL hash encoding (#config=base64...)
```

**Single-database chunked loading:**

The pip-installed hyrr and the browser hyrr use identical SQL queries against the same schema. The only difference is how data enters the database:

- **pip version**: opens `hyrr.sqlite` directly (single file, all data)
- **WASM version**: creates an empty in-memory SQLite via sql.js, then loads SQL INSERT chunks on demand

This means `db.py` needs zero changes — one code path for both environments.

**Data chunk format and loading flow:**

The build pipeline (`data/build_chunks.py`) exports the master `hyrr.sqlite` into gzipped SQL INSERT files:

```sql
-- xs/p_Mo.sql.gz (after decompression)
INSERT INTO cross_sections VALUES ('p',42,92,43,92,'',8.0,5.21,'iaea.2024');
INSERT INTO cross_sections VALUES ('p',42,92,43,92,'',8.5,6.73,'iaea.2024');
...
```

Browser-side loading sequence:

```js
// 1. Create empty database with schema (instant)
const db = new SQL.Database();
db.run(SCHEMA_SQL);  // CREATE TABLEs, no data yet

// 2. Always load meta + stopping on init (tiny, <500 KB total)
await loadChunk(db, "/data/meta.sql.gz");      // abundances + decay
await loadChunk(db, "/data/stopping.sql.gz");   // PSTAR/ASTAR

// 3. On demand — user configures Mo target with protons
await loadChunk(db, "/data/xs/p_Mo.sql.gz");

// 4. User adds Havar window → parallel fetch
await Promise.all([
    loadChunk(db, "/data/xs/p_Co.sql.gz"),
    loadChunk(db, "/data/xs/p_Cr.sql.gz"),
    loadChunk(db, "/data/xs/p_Ni.sql.gz"),
    loadChunk(db, "/data/xs/p_Fe.sql.gz"),
    loadChunk(db, "/data/xs/p_W.sql.gz"),
]);

// 5. Build indexes after loading (once, or deferred)
db.run("CREATE INDEX IF NOT EXISTS idx_reaction ON cross_sections(...)");

// Now hyrr (via Pyodide) queries this db — identical SQL to pip version
```

All fetched chunks are cached in IndexedDB — never re-downloaded on subsequent visits.
First simulation: ~1-2 MB total. Subsequent: instant from cache.

**Shareable URL configs:**

```
https://morepet.github.io/hyrr/#config=eyJiZWFtIjp7InByb2plY3...

decodes to:
{
  "beam": {"projectile": "p", "energy_MeV": 16, "current_mA": 0.15},
  "layers": [
    {"material": "havar", "thickness_cm": 0.0025},
    {"material": "Mo-100", "enrichment": 0.995, "energy_out_MeV": 12}
  ],
  "irradiation_s": 86400,
  "cooling_s": 86400
}
```

Colleague clicks link → app loads with exact config → hits run. No account needed.

**Browser-side history (IndexedDB):**

```
runs: {
  id, timestamp, label,
  config: { beam, layers, irradiation, cooling },
  results: { activities, summary, ... }
}
```

Users get a history table: re-run, compare side-by-side, export, delete. All local, no auth.

Tasks:
- [ ] Write `data/build_chunks.py` — export `hyrr.sqlite` into per-element `.sql.gz` INSERT chunks + `schema.sql` + `meta.sql.gz` + `stopping.sql.gz`
- [ ] Svelte frontend: beam config, dynamic layer builder, material selector
- [ ] Pyodide integration: load hyrr in Web Worker, run simulations off main thread
- [ ] sql.js integration: load per-element SQLite chunks on demand
- [ ] Service Worker: cache WASM bundles + nuclear data in Cache API / IndexedDB
- [ ] Plotly.js result visualization (depth profiles, cooling curves, activity tables)
- [ ] IndexedDB history store with compare/export/delete
- [ ] URL hash config encoding/decoding for shareable links
- [ ] JSON/CSV export from browser (download generated files)
- [ ] Loading UX: progress bar for first-time WASM download, instant on revisit
- [ ] GitHub Actions workflow: build Svelte → deploy to gh-pages branch
- [ ] Preset configs shipped as static JSON (Tc-99m, F-18, Ge-68, At-211, Ac-225)

**No DataFrame dependency in WASM**: since the core uses plain numpy arrays (polars/pandas are optional export-only), the WASM build has no DataFrame compatibility issues. NumPy and SciPy both have stable Pyodide builds.

#### 9c: Optional server-side extension (if needed later)

If HYRR grows into a shared platform requiring persistent cross-user features (team presets, shared history, institutional deployment), a thin server layer can be added:

- **Backend**: FastAPI + MariaDB/SQLite for user history, shared configs, team management
- **Frontend**: same Svelte app, extended with auth (ETH OIDC/Shibboleth) and sync
- **Deployment**: ETH VM or container, reverse-proxied

This is only warranted if cross-user collaboration features are needed — the serverless WASM app covers individual and link-shared use cases fully.

---

## 6. Dependencies

### Runtime
```
numpy >= 1.24
scipy >= 1.11
matplotlib >= 3.7
pymat @ git+https://github.com/MorePET/py-mat.git
```

Note: `sqlite3` is Python stdlib — no dependency. No h5py, no Fortran.

### Optional (export formats)
```
polars >= 1.0       # pip install hyrr[polars]
pandas >= 2.0       # pip install hyrr[pandas]
openpyxl >= 3.1     # pip install hyrr[excel] (for to_excel)
```

### Development
```
pytest >= 7.4
pytest-cov >= 4.1
ruff >= 0.3
mypy >= 1.10
pre-commit >= 3.6
```

### Data build (one-time)
```
# Only needed to rebuild hyrr.sqlite from raw TENDL/PSTAR files
# Not needed by end users
```

---

## 7. Migration from curie

| curie (current) | hyrr (new) |
|---|---|
| `isotopia.py` (subprocess wrapper) | `production.py` (native Python physics) |
| `models.py` (Pydantic) | `models.py` (dataclasses, extended for stacks) |
| `utils.py` (output parser) | Eliminated — no external output to parse |
| `cli.py` | `cli.py` (retained, updated) |
| `isotopia/` (2.1 GB Fortran binary + source) | Eliminated |
| `isotopia.libs/` (547K text files, 2.1 GB) | `data/hyrr.sqlite` (~200-300 MB) |
| `samples/` (reference outputs) | `tests/` (validation against known results) |
| Podman container required | `pip install hyrr` |
| Bethe-Bloch (no corrections) | PSTAR/ASTAR tables (NIST reference) |
| Single pure-element target | Compounds, enrichment, stacked layers |
| Total activity per isotope | + depth profiles, energy scans, purity |
| pandas | polars |

---

## 8. Validation strategy

Every phase must validate against known results before proceeding.

### Reference cases (from ISOTOPIA samples)
1. **p + Mo-100 → Tc-99m** (16 → 12 MeV) — the standard medical isotope case
2. **p + O-18 → F-18** — PET isotope production
3. **p + Ga-69 → Ge-68** — generator production
4. **α + Bi-209 → At-211** — alpha-emitter for therapy
5. **p + Ra-226 → Ac-225** — emerging therapeutic isotope

### Validation levels
- **Stopping power**: compare against NIST PSTAR web calculator (must match to <0.1%)
- **Production rates**: compare against ISOTOPIA output (accept ≤5% deviation, document cause — improved stopping power)
- **Bateman equations**: compare activity/yield values against ISOTOPIA for all isotopes in reference cases
- **Compound targets**: cross-check against literature values for MoO₃, natural element targets
- **Stack geometry**: verify energy propagation by comparing single-layer results with manually computed multi-layer decomposition

### Regression test suite
- Each reference case becomes a pytest fixture
- CI runs full validation on every commit
- Deviations from reference values trigger test failure with clear diagnostics

---

## 9. Open questions

1. **TENDL version pinning**: Ship hyrr.sqlite built from TENDL-2023 (current in isotopia.libs). How to handle future TENDL releases? Rebuild script + versioned database files?
2. **EXFOR experimental data**: The isotopia.libs also contain EXFOR measured cross-sections. Include in SQLite as separate source? Useful for validation/comparison plots but not needed for production calculations.
3. **Database distribution**: ~200-300 MB SQLite is too large for PyPI (100 MB limit). Partially solved by the WASM lazy-loading architecture (Phase 9b) — the same chunked approach works for pip users: `hyrr download-data` can fetch per-element chunks from GitHub Pages/release assets and assemble locally, or fetch a single file from a GitHub release. No need for a separate distribution mechanism.
5. **Non-TENDL cross-sections**: Some users may want to use their own measured cross-sections or alternative libraries (ENDF, JENDL). Plugin architecture for data sources?
6. **Natural element targets**: ISOTOPIA's `mass 0` mode runs all stable isotopes of an element separately. With compounds + natural abundances, hyrr handles this automatically. Verify equivalence.
7. **Temperature/pressure dependent density**: py-mat has factory functions for this (water, air). Relevant for gas targets? Niche but possible.

---

## 10. Future features (parked)

### MSTAR alpha stopping power coefficients

The libdEdx repository contains `MSTAR.dat` / `mstarEng.dat` — alpha stopping power data covering 4 elements beyond ASTAR (B Z=5, Ni Z=28, Zr Z=40, Ta Z=73). However, **MSTAR files contain raw coefficients, not direct stopping powers**. They require M. Paul effective charge scaling implemented in libdEdx's C code (`_dedx_calculate_mspaul_coef()`). Without porting that formula to Python, the raw values are unusable (~99% deviation from ASTAR). Currently these elements are handled by the pycatima/SRIM fallback, so MSTAR adds no coverage benefit. If the M. Paul scaling is ever needed, the implementation lives in `libdedx/dedx_mstar.c`.

### ICRU73 heavy-ion stopping power

Downloaded from libdEdx: `ICRU73.dat` / `icru73Eng.dat` (896 blocks, 53 energy points each). Covers heavy ions with projectile A=3→18 (Li through Ar) on ~25 target elements. Format: `#target_Z:projectile_A:NPTS`. Integration would require:

1. Extending the `stopping_power` DB table with a `projectile_A` column
2. Extending `stopping.py` to handle arbitrary projectile masses beyond p/d/t/h/a
3. Extending `models.py` `ProjectileType` to support heavy ions

This is relevant for heavy-ion therapy research and exotic isotope production but outside the current scope (proton/deuteron/triton/He-3/alpha beams).
