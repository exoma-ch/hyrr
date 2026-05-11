CREATE TABLE IF NOT EXISTS cross_sections (
    projectile  TEXT    NOT NULL,
    target_Z    INTEGER NOT NULL,
    target_A    INTEGER NOT NULL,
    residual_Z  INTEGER NOT NULL,
    residual_A  INTEGER NOT NULL,
    state       TEXT    NOT NULL DEFAULT '',
    energy_MeV  REAL    NOT NULL,
    xs_mb       REAL    NOT NULL,
    source      TEXT    NOT NULL DEFAULT 'iaea.2024',
    PRIMARY KEY (projectile, target_Z, target_A,
                 residual_Z, residual_A, state, energy_MeV, source)
);
CREATE INDEX IF NOT EXISTS idx_reaction
    ON cross_sections(projectile, target_Z, target_A);
CREATE INDEX IF NOT EXISTS idx_product
    ON cross_sections(residual_Z, residual_A, state);

CREATE TABLE IF NOT EXISTS stopping_power (
    source      TEXT    NOT NULL,
    target_Z    INTEGER NOT NULL,
    energy_MeV  REAL    NOT NULL,
    dedx        REAL    NOT NULL,
    PRIMARY KEY (source, target_Z, energy_MeV)
);

CREATE TABLE IF NOT EXISTS natural_abundances (
    Z            INTEGER NOT NULL,
    A            INTEGER NOT NULL,
    symbol       TEXT    NOT NULL,
    abundance    REAL    NOT NULL,
    atomic_mass  REAL    NOT NULL,
    PRIMARY KEY (Z, A)
);

CREATE TABLE IF NOT EXISTS decay_data (
    Z              INTEGER NOT NULL,
    A              INTEGER NOT NULL,
    state          TEXT    NOT NULL DEFAULT '',
    half_life_s    REAL,
    decay_mode     TEXT    NOT NULL,
    daughter_Z     INTEGER,
    daughter_A     INTEGER,
    daughter_state TEXT    DEFAULT '',
    branching      REAL    DEFAULT 1.0,
    PRIMARY KEY (Z, A, state, decay_mode, daughter_Z, daughter_A, daughter_state)
);
CREATE INDEX IF NOT EXISTS idx_parent
    ON decay_data(Z, A, state);
CREATE INDEX IF NOT EXISTS idx_daughter
    ON decay_data(daughter_Z, daughter_A, daughter_state);

CREATE TABLE IF NOT EXISTS elements (
    Z       INTEGER PRIMARY KEY,
    symbol  TEXT    NOT NULL UNIQUE
);
