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
| DataFrames | Polars | Faster, better API, new project — no legacy pandas baggage |
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
    activity: pl.DataFrame       # time vs activity (irradiation + cooling)
    yield_curve: pl.DataFrame    # time vs yield [Bq/mAh]

@dataclass
class StackResult:
    stack: TargetStack
    layers: list[LayerResult]
    irradiation_time_s: float
    cooling_time_s: float

    def purity_at(self, cooling_time_s: float, isotope: str) -> float:
        """Fraction of total activity from specified isotope at given cooling time."""

    def to_polars(self) -> pl.DataFrame:
        """All results as a single DataFrame."""

    def to_excel(self, path: str) -> None:
        """Formatted export for logbook/reporting."""

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

### Phase 8: Output & reporting

**Goal**: Get results out of notebooks and into papers/reports.

Tasks:
- [ ] `result.to_polars()` — all results as Polars DataFrames
- [ ] `result.summary()` — text output similar to ISOTOPIA format (for comparison/validation)
- [ ] `result.to_excel()` — formatted spreadsheet for logbook/regulatory use
- [ ] Plotting module (`plotting.py`):
  - Depth profiles (heat, activity) with layer annotations
  - Activity vs time (irradiation + cooling) per isotope
  - Energy scan results
  - Purity vs cooling time
  - Support both matplotlib (publication) and plotly (interactive/notebook)

---

## 6. Dependencies

### Runtime
```
numpy >= 1.24
scipy >= 1.11
polars >= 1.0
matplotlib >= 3.7
pymat @ git+https://github.com/MorePET/py-mat.git
```

Note: `sqlite3` is Python stdlib — no dependency. No h5py, no pandas, no Fortran.

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
3. **Database distribution**: ~200-300 MB SQLite is too large for PyPI (100 MB limit). Options: separate data package, GitHub release asset, or zenodo DOI with download-on-first-use.
4. **Decay chain depth**: ISOTOPIA tracks one generation of parent→daughter decay. Extend to full chains? Relevant for long-lived daughters (e.g., Ac-225 → Fr-221 → At-217 → ...).
5. **Non-TENDL cross-sections**: Some users may want to use their own measured cross-sections or alternative libraries (ENDF, JENDL). Plugin architecture for data sources?
6. **Natural element targets**: ISOTOPIA's `mass 0` mode runs all stable isotopes of an element separately. With compounds + natural abundances, hyrr handles this automatically. Verify equivalence.
7. **Temperature/pressure dependent density**: py-mat has factory functions for this (water, air). Relevant for gas targets? Niche but possible.
