//! Structured-export tables for the #427 dataset tools, with the
//! self-describing schema of #569.
//!
//! A [`Table`] is a set of named, typed columns in long format. Every column
//! carries a [`ColSpec`] (unit + one-line description + evaluation point, plus
//! per-column null-meaning where relevant) that feeds BOTH:
//!
//!   * the inline JSON — via [`Table::schema_json`] alongside
//!     [`Table::to_json_rows`], so an LLM reading the tool text sees the same
//!     metadata as a downstream Parquet consumer, and
//!   * the embedded Parquet — via Arrow field-level metadata and file-level
//!     `key_value_metadata` (dataset-level provenance in [`DatasetMeta`]).
//!
//! One column source, one metadata source — the two representations cannot
//! drift. That is the #569 acceptance criterion.
//!
//! Builders turn a [`StackResult`] (+ decay/emission data) into the
//! inventory / cooling-tail / depth-profile / emission tables the issues spell
//! out.

use serde_json::{Map, Number, Value};

use crate::db::DatabaseProtocol;
use crate::types::{EmissionLine, IsotopeResult, StackResult};

/// Where in the simulation lifecycle a column's value is evaluated. Attached
/// to every [`ColSpec`] so a fresh consumer can tell `activity_at_eob_bq`
/// (single-point, at end of bombardment) from `activity_bq` in the cooling
/// table (one row per time-grid sample) without reading the column names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvalPoint {
    /// Identifier or time-independent property (`simulation_id`, `z`, `a`,
    /// `half_life_s`, branching fractions, `energy_kev`, …).
    Static,
    /// Evaluated at the first time-grid sample at or after the irradiation
    /// time — the "end of bombardment" convention shared with the cooling
    /// curve tools.
    EndOfBombardment,
    /// Evaluated at the last time-grid sample (`t_irr + cooling_time_s`).
    EndOfCooling,
    /// One row per time-grid sample; column carries the per-row value.
    PerTimeGridRow,
    /// One row per depth-profile sample; column carries the per-row value.
    PerDepthRow,
}

impl EvalPoint {
    /// Canonical string tag written into schema metadata (JSON + Parquet).
    /// Stable — clients may branch on this.
    pub const fn as_str(self) -> &'static str {
        match self {
            EvalPoint::Static => "static",
            EvalPoint::EndOfBombardment => "end_of_bombardment",
            EvalPoint::EndOfCooling => "end_of_cooling",
            EvalPoint::PerTimeGridRow => "per_time_grid_row",
            EvalPoint::PerDepthRow => "per_depth_row",
        }
    }
}

/// Self-describing schema for one column (#569). Attached to every [`Col`]
/// so units, meaning, and evaluation point ship with the data — in the inline
/// JSON, in the Parquet's Arrow field metadata, and (dataset-level) in the
/// Parquet file's `key_value_metadata`.
#[derive(Clone, Copy, Debug)]
pub struct ColSpec {
    pub name: &'static str,
    /// Canonical unit, e.g. `"Bq"`, `"s"`, `"keV"`, `"MeV"`, `"cm"`, `"1"`
    /// (dimensionless / a branching fraction), or `""` for an identifier or
    /// categorical column. Always populated — even where the column name
    /// carries the unit — because "the implicit convention is exactly what
    /// a fresh agent gets wrong" (#569).
    pub unit: &'static str,
    /// One-line description; renders alongside the column in the schema block.
    pub description: &'static str,
    pub eval_point: EvalPoint,
    /// For nullable columns only: what a null means in that column
    /// (`"no dose constant available" ≠ "zero dose"`, per #569). `None` on
    /// non-nullable columns.
    pub null_meaning: Option<&'static str>,
}

impl ColSpec {
    /// Non-nullable column spec.
    pub const fn new(
        name: &'static str,
        unit: &'static str,
        description: &'static str,
        eval_point: EvalPoint,
    ) -> Self {
        Self {
            name,
            unit,
            description,
            eval_point,
            null_meaning: None,
        }
    }

    /// Nullable column spec — `null_meaning` documents what a null denotes so
    /// consumers don't confuse "not measured / unknown" with a numeric zero.
    pub const fn nullable(
        name: &'static str,
        unit: &'static str,
        description: &'static str,
        eval_point: EvalPoint,
        null_meaning: &'static str,
    ) -> Self {
        Self {
            name,
            unit,
            description,
            eval_point,
            null_meaning: Some(null_meaning),
        }
    }

    /// Metadata as a JSON object — the inline-response mirror of the
    /// Parquet field-level metadata. Keys stay stable across releases.
    fn to_json(self) -> Value {
        let mut m = Map::new();
        m.insert("name".to_string(), Value::String(self.name.to_string()));
        m.insert("unit".to_string(), Value::String(self.unit.to_string()));
        m.insert(
            "description".to_string(),
            Value::String(self.description.to_string()),
        );
        m.insert(
            "eval_point".to_string(),
            Value::String(self.eval_point.as_str().to_string()),
        );
        m.insert(
            "nullable".to_string(),
            Value::Bool(self.null_meaning.is_some()),
        );
        if let Some(meaning) = self.null_meaning {
            m.insert(
                "null_meaning".to_string(),
                Value::String(meaning.to_string()),
            );
        }
        Value::Object(m)
    }

    /// Field-level metadata for the Arrow schema. Round-trips through Parquet
    /// via the embedded Arrow schema (`ARROW:schema` in file KV) and is
    /// visible to any Arrow-based reader (polars, pandas via pyarrow, …).
    fn to_arrow_metadata(self) -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("hyrr.unit".to_string(), self.unit.to_string());
        m.insert("hyrr.description".to_string(), self.description.to_string());
        m.insert(
            "hyrr.eval_point".to_string(),
            self.eval_point.as_str().to_string(),
        );
        if let Some(meaning) = self.null_meaning {
            m.insert("hyrr.null_meaning".to_string(), meaning.to_string());
        }
        m
    }
}

/// One typed column. Nullable variants map to Parquet nullable + JSON `null`.
/// Every variant carries a [`ColSpec`] so schema metadata travels with the
/// data and cannot drift from it.
pub enum Col {
    I64(ColSpec, Vec<i64>),
    OptI64(ColSpec, Vec<Option<i64>>),
    F64(ColSpec, Vec<f64>),
    OptF64(ColSpec, Vec<Option<f64>>),
    Str(ColSpec, Vec<String>),
    OptStr(ColSpec, Vec<Option<String>>),
}

impl Col {
    pub fn spec(&self) -> &ColSpec {
        match self {
            Col::I64(s, _)
            | Col::OptI64(s, _)
            | Col::F64(s, _)
            | Col::OptF64(s, _)
            | Col::Str(s, _)
            | Col::OptStr(s, _) => s,
        }
    }

    pub fn name(&self) -> &'static str {
        self.spec().name
    }

    fn len(&self) -> usize {
        match self {
            Col::I64(_, v) => v.len(),
            Col::OptI64(_, v) => v.len(),
            Col::F64(_, v) => v.len(),
            Col::OptF64(_, v) => v.len(),
            Col::Str(_, v) => v.len(),
            Col::OptStr(_, v) => v.len(),
        }
    }

    fn json_at(&self, i: usize) -> Value {
        let f64_to_json = |x: f64| {
            Number::from_f64(x)
                .map(Value::Number)
                .unwrap_or(Value::Null)
        };
        match self {
            Col::I64(_, v) => Value::Number(v[i].into()),
            Col::OptI64(_, v) => v[i].map(|x| Value::Number(x.into())).unwrap_or(Value::Null),
            Col::F64(_, v) => f64_to_json(v[i]),
            Col::OptF64(_, v) => v[i].map(f64_to_json).unwrap_or(Value::Null),
            Col::Str(_, v) => Value::String(v[i].clone()),
            Col::OptStr(_, v) => v[i].clone().map(Value::String).unwrap_or(Value::Null),
        }
    }

    /// Sort key for `sort_by` on inline JSON. Returns `None` for non-numeric
    /// columns and for missing values in nullable columns — sort direction
    /// (descending in [`Table::inline_row_order`]) then treats those rows as
    /// "smallest" so they land at the bottom (not silently dropped).
    fn f64_at(&self, i: usize) -> Option<f64> {
        match self {
            Col::F64(_, v) => Some(v[i]),
            Col::OptF64(_, v) => v[i],
            Col::I64(_, v) => Some(v[i] as f64),
            Col::OptI64(_, v) => v[i].map(|x| x as f64),
            Col::Str(_, _) | Col::OptStr(_, _) => None,
        }
    }
}

/// Dataset-level provenance (#569 P0). Written into the Parquet file's
/// file-level `key_value_metadata` (visible to any Parquet reader including
/// DuckDB's `parquet_kv_metadata`) and mirrored in the tool JSON, so a
/// Parquet that outlives the conversation stays self-contained.
#[derive(Clone, Debug)]
pub struct DatasetMeta {
    /// Config-hashed simulation id (same value as the `hyrr://sim/<id>/…` URI).
    pub simulation_id: String,
    /// `hyrr-core` version at emit time (from `CARGO_PKG_VERSION`).
    pub core_version: &'static str,
    /// Loaded nuclear data library id (e.g. `tendl-2023-iso`).
    pub library: String,
    /// The full simulation config as canonical JSON — the exact `args` the
    /// tool was called with. Consumers can round-trip it to reproduce.
    pub config_json: String,
    /// Shared time grid [s], if the sim has one; empty when no producing
    /// isotope was found (an empty table still gets provenance).
    pub time_grid_s: Vec<f64>,
    pub irradiation_time_s: f64,
    pub cooling_time_s: f64,
    /// Emitting table's name (e.g. `"inventory"`), duplicated into the file
    /// KV so a Parquet detached from its URI still identifies itself.
    pub table_name: String,
}

impl DatasetMeta {
    /// Mirror-of-Parquet-KV JSON for the inline response. Keys match the
    /// file-level `key_value_metadata` values 1:1 (parsed where they carry
    /// JSON, so a consumer doesn't have to re-parse). `table_name` is
    /// deliberately omitted here — it's per-Parquet-file and only meaningful
    /// on the individual resource's KV, not on the dataset-shared provenance
    /// block that heads the response.
    pub fn to_json(&self) -> Value {
        let config = serde_json::from_str::<Value>(&self.config_json)
            .unwrap_or_else(|_| Value::String(self.config_json.clone()));
        serde_json::json!({
            "simulation_id": self.simulation_id,
            "hyrr_core_version": self.core_version,
            "library": self.library,
            "irradiation_time_s": self.irradiation_time_s,
            "cooling_time_s": self.cooling_time_s,
            "time_grid_s": self.time_grid_s,
            "config": config,
        })
    }

    /// Flat key/value pairs for Parquet's file-level `key_value_metadata`.
    /// One key per fact — arrays / structured values are JSON-encoded so any
    /// reader can pick them up as raw strings and re-parse if needed.
    fn to_parquet_kv(&self) -> Vec<(String, String)> {
        vec![
            ("hyrr.simulation_id".into(), self.simulation_id.clone()),
            ("hyrr.core_version".into(), self.core_version.to_string()),
            ("hyrr.library".into(), self.library.clone()),
            ("hyrr.table_name".into(), self.table_name.clone()),
            (
                "hyrr.irradiation_time_s".into(),
                self.irradiation_time_s.to_string(),
            ),
            (
                "hyrr.cooling_time_s".into(),
                self.cooling_time_s.to_string(),
            ),
            (
                "hyrr.time_grid_s_json".into(),
                serde_json::to_string(&self.time_grid_s).unwrap_or_else(|_| "[]".into()),
            ),
            ("hyrr.config_json".into(), self.config_json.clone()),
        ]
    }
}

/// A named long-format table.
pub struct Table {
    pub name: &'static str,
    pub cols: Vec<Col>,
}

impl Table {
    pub fn nrows(&self) -> usize {
        self.cols.first().map(Col::len).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.nrows() == 0
    }

    /// Column-level schema metadata as JSON — the inline mirror of the
    /// Parquet field metadata. One object per column, in schema order.
    pub fn schema_json(&self) -> Value {
        Value::Array(self.cols.iter().map(|c| c.spec().to_json()).collect())
    }

    /// Full long-format rows as JSON objects (one per row). No truncation —
    /// use [`Self::inline_json_rows`] for the token-bounded view.
    pub fn to_json_rows(&self) -> Vec<Value> {
        (0..self.nrows()).map(|i| self.row_at(i)).collect()
    }

    fn row_at(&self, i: usize) -> Value {
        let mut obj = Map::new();
        for col in &self.cols {
            obj.insert(col.name().to_string(), col.json_at(i));
        }
        Value::Object(obj)
    }

    /// Row indices in the order chosen for the inline JSON view:
    /// descending by `sort_by` when supplied (F64/OptF64/I64/OptI64 only);
    /// otherwise the natural insertion order. Non-numeric or missing sort
    /// values land at the bottom — sorted, never dropped.
    ///
    /// Returns `Err` if `sort_by` names a column that doesn't exist or that
    /// isn't numeric; the caller surfaces the error rather than silently
    /// falling back (would be a #533-shaped silent-loss regression).
    pub fn inline_row_order(&self, sort_by: Option<&str>) -> Result<Vec<usize>, String> {
        let mut order: Vec<usize> = (0..self.nrows()).collect();
        let Some(key) = sort_by else {
            return Ok(order);
        };
        let col = self.cols.iter().find(|c| c.name() == key).ok_or_else(|| {
            format!(
                "sort_by: unknown column '{key}' in table '{}'. Available: {}",
                self.name,
                self.cols
                    .iter()
                    .map(|c| c.name())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
        // Refuse a non-numeric sort key — silent no-op would recreate #533.
        if !matches!(
            col,
            Col::F64(_, _) | Col::OptF64(_, _) | Col::I64(_, _) | Col::OptI64(_, _)
        ) {
            return Err(format!(
                "sort_by: column '{key}' is not numeric (only F64 / OptF64 / I64 / OptI64 \
                 columns support sort_by)"
            ));
        }
        order.sort_by(|&a, &b| {
            let av = col.f64_at(a);
            let bv = col.f64_at(b);
            // Descending: larger first; None sorts as "smallest" → bottom.
            match (av, bv) {
                (Some(x), Some(y)) => y.partial_cmp(&x).unwrap_or(std::cmp::Ordering::Equal),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
        });
        Ok(order)
    }

    /// Token-bounded inline view: rows ordered by `sort_by` (see
    /// [`Self::inline_row_order`]) and truncated to `top_n` (when `Some`).
    /// The Parquet resource is emitted separately from the full column data
    /// and is unaffected — this is the #569 inline-only bound.
    ///
    /// Returns `(rows, truncated_from_total_of)` where the second element is
    /// `Some(total)` if any row was omitted, so the caller can surface the
    /// truncation.
    pub fn inline_json_rows(
        &self,
        top_n: Option<usize>,
        sort_by: Option<&str>,
    ) -> Result<(Vec<Value>, Option<usize>), String> {
        let order = self.inline_row_order(sort_by)?;
        let total = order.len();
        let take = top_n.map(|n| n.min(total)).unwrap_or(total);
        let rows = order.iter().take(take).map(|&i| self.row_at(i)).collect();
        let truncated = (take < total).then_some(total);
        Ok((rows, truncated))
    }

    /// Serialize to in-memory Parquet bytes via Arrow, attaching:
    ///   * per-column metadata (unit, description, eval_point, null_meaning)
    ///     as Arrow field metadata — round-trips through polars/pandas, and
    ///   * dataset-level provenance (`meta`, when `Some`) as file-level
    ///     `key_value_metadata` on the Parquet file — visible to *any*
    ///     Parquet reader (DuckDB, pyarrow, parquet-tools), whether or not
    ///     it understands the Arrow schema. Field metadata is redundant with
    ///     the Arrow schema for Arrow-aware readers; this dual channel keeps
    ///     the Parquet self-describing for both worlds.
    pub fn to_parquet_bytes(&self, meta: Option<&DatasetMeta>) -> Result<Vec<u8>, String> {
        use arrow::array::{ArrayRef, Float64Array, Int64Array, StringArray};
        use arrow::datatypes::{DataType, Field, Schema};
        use arrow::record_batch::RecordBatch;
        use parquet::arrow::ArrowWriter;
        use parquet::format::KeyValue;
        use std::sync::Arc;

        let mut fields = Vec::with_capacity(self.cols.len());
        let mut arrays: Vec<ArrayRef> = Vec::with_capacity(self.cols.len());
        for col in &self.cols {
            let (mut field, array): (Field, ArrayRef) = match col {
                Col::I64(s, v) => (
                    Field::new(s.name, DataType::Int64, false),
                    Arc::new(Int64Array::from(v.clone())),
                ),
                Col::OptI64(s, v) => (
                    Field::new(s.name, DataType::Int64, true),
                    Arc::new(Int64Array::from(v.clone())),
                ),
                Col::F64(s, v) => (
                    Field::new(s.name, DataType::Float64, false),
                    Arc::new(Float64Array::from(v.clone())),
                ),
                Col::OptF64(s, v) => (
                    Field::new(s.name, DataType::Float64, true),
                    Arc::new(Float64Array::from(v.clone())),
                ),
                Col::Str(s, v) => (
                    Field::new(s.name, DataType::Utf8, false),
                    Arc::new(StringArray::from_iter(v.iter().map(|s| Some(s.as_str())))),
                ),
                Col::OptStr(s, v) => (
                    Field::new(s.name, DataType::Utf8, true),
                    Arc::new(StringArray::from_iter(v.iter().map(|o| o.as_deref()))),
                ),
            };
            field.set_metadata(col.spec().to_arrow_metadata());
            fields.push(field);
            arrays.push(array);
        }

        // Schema-level Arrow metadata — a compact `hyrr.table_name` tag so
        // the Arrow schema round-trips a self-identifier too. Dataset-level
        // provenance goes into the file-level Parquet KV below (visible to
        // non-Arrow readers).
        let mut schema_meta = std::collections::HashMap::new();
        schema_meta.insert("hyrr.table_name".to_string(), self.name.to_string());
        if let Some(m) = meta {
            schema_meta.insert("hyrr.simulation_id".to_string(), m.simulation_id.clone());
            schema_meta.insert("hyrr.core_version".to_string(), m.core_version.to_string());
            schema_meta.insert("hyrr.library".to_string(), m.library.clone());
        }
        let schema = Arc::new(Schema::new_with_metadata(fields, schema_meta));
        let batch = RecordBatch::try_new(Arc::clone(&schema), arrays).map_err(|e| e.to_string())?;

        let mut buf = Vec::new();
        {
            let mut writer =
                ArrowWriter::try_new(&mut buf, schema, None).map_err(|e| e.to_string())?;
            writer.write(&batch).map_err(|e| e.to_string())?;
            if let Some(m) = meta {
                for (k, v) in m.to_parquet_kv() {
                    writer.append_key_value_metadata(KeyValue::new(k, v));
                }
            }
            writer.close().map_err(|e| e.to_string())?;
        }
        Ok(buf)
    }
}

/// β+ / EC / β- / IT branching fractions for a nuclide, summed across the
/// sub-modes the data splits EC into (`KshellEC`, `LshellEC`, …).
pub fn branching_split(
    db: &dyn DatabaseProtocol,
    z: u32,
    a: u32,
    state: &str,
) -> (f64, f64, f64, f64) {
    let (mut bp, mut ec, mut bm, mut it) = (0.0, 0.0, 0.0, 0.0);
    if let Some(decay) = db.get_decay_data(z, a, state) {
        for m in &decay.decay_modes {
            match m.mode.as_str() {
                "beta+" => bp += m.branching,
                "beta-" => bm += m.branching,
                "IT" => it += m.branching,
                s if s.ends_with("shellEC") || s == "EC" => ec += m.branching,
                _ => {}
            }
        }
    }
    (bp, ec, bm, it)
}

/// Activity [Bq] at end of bombardment — the first time-grid sample at or after
/// the irradiation time (matching the cooling-curve tool's EOB convention).
pub fn eob_activity(iso: &IsotopeResult, t_irr: f64) -> f64 {
    iso.time_grid_s
        .iter()
        .position(|&t| t >= t_irr)
        .and_then(|i| iso.activity_vs_time_bq.get(i).copied())
        .unwrap_or(iso.activity_bq)
}

/// Sorted, de-duplicated list of every isotope produced anywhere in the stack,
/// as `(z, a, state, name, total_activity_bq)` — `total_activity_bq` is the
/// end-of-cooling activity summed across every producing layer. Used by the
/// (layer-independent) emission table and its `activity_floor_bq` filter (#567).
fn distinct_isotopes(result: &StackResult) -> Vec<(u32, u32, String, String, f64)> {
    let mut seen: std::collections::BTreeMap<(u32, u32, String), (String, f64)> =
        std::collections::BTreeMap::new();
    for lr in &result.layer_results {
        for iso in lr.isotope_results.values() {
            let entry = seen
                .entry((iso.z, iso.a, iso.state.clone()))
                .or_insert_with(|| (iso.name.clone(), 0.0));
            entry.1 += iso.activity_bq;
        }
    }
    seen.into_iter()
        .map(|((z, a, st), (name, act))| (z, a, st, name, act))
        .collect()
}

/// Result of building a filtered table: the table itself plus how many rows
/// were dropped by the reporting-layer `activity_floor_bq` filter (issue #567).
/// The count is surfaced by every inventory-derived tool so filtering is never
/// silent — same no-silent-loss contract as `pruned_negligible_count`.
pub struct FilteredTable {
    pub table: Table,
    pub filtered_below_floor: usize,
}

/// Does this isotope pass the caller's absolute activity floor? Applied to
/// the end-of-cooling activity `iso.activity_bq` (matches the GUI's `activity`
/// threshold semantics from `display-thresholds.svelte.ts`). A floor of 0 is
/// the sentinel "no filtering" — checked as a fast path first so the common
/// case is one comparison, not a subtraction under a threshold.
#[inline]
pub fn passes_activity_floor(iso: &IsotopeResult, activity_floor_bq: f64) -> bool {
    activity_floor_bq <= 0.0 || iso.activity_bq >= activity_floor_bq
}

// ─── Column specs ──────────────────────────────────────────────────────────
//
// Every column spec lives at exactly one site — the builder that populates
// it — so the shared-source invariant (#569 "extend the existing shared-source
// design; do NOT add a parallel hand-maintained table that can drift") holds.
// The `const`s below are named to make the metadata scannable at build time
// and are referenced only by the builder that owns them.

// -- Inventory table -------------------------------------------------------

const INV_SIM_ID: ColSpec = ColSpec::new(
    "simulation_id",
    "",
    "Config-hashed simulation id (matches the `hyrr://sim/<id>/…` resource URI)",
    EvalPoint::Static,
);
const INV_LAYER_INDEX: ColSpec = ColSpec::new(
    "layer_index",
    "",
    "1-based layer index in beam-traversal order",
    EvalPoint::Static,
);
const INV_LAYER_MATERIAL: ColSpec = ColSpec::new(
    "layer_material",
    "",
    "Material as written in the caller's config for this layer",
    EvalPoint::Static,
);
const INV_Z: ColSpec = ColSpec::new("z", "", "Residual-nuclide atomic number", EvalPoint::Static);
const INV_A: ColSpec = ColSpec::new("a", "", "Residual-nuclide mass number", EvalPoint::Static);
const INV_STATE: ColSpec = ColSpec::new(
    "state",
    "",
    "Nuclear state ('' = ground, 'm'/'m1'/'m2' = metastable)",
    EvalPoint::Static,
);
const INV_ISOTOPE: ColSpec = ColSpec::new(
    "isotope",
    "",
    "Isotope name, e.g. 'F-18' or 'Sc-44m'",
    EvalPoint::Static,
);
const INV_PRODUCTION_SOURCE: ColSpec = ColSpec::new(
    "production_source",
    "",
    "How this isotope is populated: 'direct' (reaction product), 'daughter' (from decay ingrowth), or 'both'",
    EvalPoint::Static,
);
const INV_PRODUCTION_RATE: ColSpec = ColSpec::new(
    "production_rate_per_s",
    "atoms/s",
    "Direct production rate in this layer (0 for pure daughter ingrowth)",
    EvalPoint::Static,
);
const INV_SATURATION_YIELD: ColSpec = ColSpec::new(
    "saturation_yield_bq_per_ua",
    "Bq/µA",
    "Saturation yield normalised to 1 µA beam current",
    EvalPoint::Static,
);
const INV_ACT_EOB: ColSpec = ColSpec::new(
    "activity_at_eob_bq",
    "Bq",
    "Activity at end of bombardment (first time-grid sample at or after t_irr)",
    EvalPoint::EndOfBombardment,
);
const INV_ACT_COOLING: ColSpec = ColSpec::new(
    "activity_at_cooling_bq",
    "Bq",
    "Activity at end of cooling (t_irr + cooling_time_s); includes ingrowth from parents",
    EvalPoint::EndOfCooling,
);
const INV_HALF_LIFE: ColSpec = ColSpec::nullable(
    "half_life_s",
    "s",
    "Isotope half-life; null when the active library has no decay entry (treat as effectively stable, NOT as t½=0)",
    EvalPoint::Static,
    "no decay data in the active library (isotope treated as effectively stable — not a zero half-life)",
);
const INV_BETA_PLUS: ColSpec = ColSpec::new(
    "beta_plus_branching",
    "1",
    "Summed β+ branching fraction (dimensionless, 0..1)",
    EvalPoint::Static,
);
const INV_EC: ColSpec = ColSpec::new(
    "ec_branching",
    "1",
    "Summed electron-capture branching fraction (K/L/M shells combined)",
    EvalPoint::Static,
);
const INV_BETA_MINUS: ColSpec = ColSpec::new(
    "beta_minus_branching",
    "1",
    "Summed β- branching fraction",
    EvalPoint::Static,
);
const INV_IT: ColSpec = ColSpec::new(
    "it_branching",
    "1",
    "Summed isomeric-transition branching fraction",
    EvalPoint::Static,
);

/// Inventory table: one row per (isotope × layer × source).
///
/// `activity_floor_bq` is a REPORTING-layer filter (#567) — anything below it
/// is omitted from the returned table, and the drop count is returned alongside
/// so the caller can show it. Pass `0.0` for the pre-#567 behavior (no filter).
pub fn build_inventory(
    db: &dyn DatabaseProtocol,
    result: &StackResult,
    layer_materials: &[String],
    sim_id: &str,
    activity_floor_bq: f64,
) -> FilteredTable {
    let t_irr = result.irradiation_time_s;
    let (mut sid, mut li, mut mat) = (vec![], vec![], vec![]);
    let (mut z, mut a, mut state, mut iso_name) = (vec![], vec![], vec![], vec![]);
    let (mut src, mut rate, mut sat) = (vec![], vec![], vec![]);
    let (mut eob, mut cool, mut hl) = (vec![], vec![], vec![]);
    let (mut bp, mut ec, mut bm, mut it) = (vec![], vec![], vec![], vec![]);
    let mut filtered_below_floor = 0usize;

    for (idx, lr) in result.layer_results.iter().enumerate() {
        let mut isos: Vec<&IsotopeResult> = lr.isotope_results.values().collect();
        isos.sort_by(|x, y| {
            y.activity_bq
                .partial_cmp(&x.activity_bq)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| x.name.cmp(&y.name))
        });
        for iso in isos {
            if !passes_activity_floor(iso, activity_floor_bq) {
                filtered_below_floor += 1;
                continue;
            }
            sid.push(sim_id.to_string());
            li.push((idx + 1) as i64);
            mat.push(layer_materials.get(idx).cloned().unwrap_or_default());
            z.push(iso.z as i64);
            a.push(iso.a as i64);
            state.push(iso.state.clone());
            iso_name.push(iso.name.clone());
            src.push(iso.source.clone());
            rate.push(iso.production_rate);
            sat.push(iso.saturation_yield_bq_ua);
            eob.push(eob_activity(iso, t_irr));
            cool.push(iso.activity_bq);
            hl.push(iso.half_life_s);
            let (b_p, b_ec, b_m, b_it) = branching_split(db, iso.z, iso.a, &iso.state);
            bp.push(b_p);
            ec.push(b_ec);
            bm.push(b_m);
            it.push(b_it);
        }
    }

    let table = Table {
        name: "inventory",
        cols: vec![
            Col::Str(INV_SIM_ID, sid),
            Col::I64(INV_LAYER_INDEX, li),
            Col::Str(INV_LAYER_MATERIAL, mat),
            Col::I64(INV_Z, z),
            Col::I64(INV_A, a),
            Col::Str(INV_STATE, state),
            Col::Str(INV_ISOTOPE, iso_name),
            Col::Str(INV_PRODUCTION_SOURCE, src),
            Col::F64(INV_PRODUCTION_RATE, rate),
            Col::F64(INV_SATURATION_YIELD, sat),
            Col::F64(INV_ACT_EOB, eob),
            Col::F64(INV_ACT_COOLING, cool),
            Col::OptF64(INV_HALF_LIFE, hl),
            Col::F64(INV_BETA_PLUS, bp),
            Col::F64(INV_EC, ec),
            Col::F64(INV_BETA_MINUS, bm),
            Col::F64(INV_IT, it),
        ],
    };
    FilteredTable {
        table,
        filtered_below_floor,
    }
}

// -- Cooling-tail table ----------------------------------------------------

const COOL_SIM_ID: ColSpec = INV_SIM_ID;
const COOL_LAYER_INDEX: ColSpec = INV_LAYER_INDEX;
const COOL_ISOTOPE: ColSpec = INV_ISOTOPE;
const COOL_T_S: ColSpec = ColSpec::new(
    "t_s",
    "s",
    "Time coordinate on the shared time grid (per-row; t ≥ irradiation_time_s in this table)",
    EvalPoint::PerTimeGridRow,
);
const COOL_ACTIVITY: ColSpec = ColSpec::new(
    "activity_bq",
    "Bq",
    "Isotope activity at row's `t_s` (includes ingrowth from parents)",
    EvalPoint::PerTimeGridRow,
);

/// Cooling-tail table: activity [Bq] vs time for t ≥ irradiation time, one row
/// per (isotope × layer × time point). Rows whose owning isotope is below
/// `activity_floor_bq` (per #567) are omitted, and the isotope-drop count is
/// reported so the filter is never silent.
pub fn build_cooling(result: &StackResult, sim_id: &str, activity_floor_bq: f64) -> FilteredTable {
    let t_irr = result.irradiation_time_s;
    let (mut sid, mut li, mut iso_name, mut t, mut act) = (vec![], vec![], vec![], vec![], vec![]);
    let mut filtered_below_floor = 0usize;
    for (idx, lr) in result.layer_results.iter().enumerate() {
        for iso in lr.isotope_results.values() {
            if !passes_activity_floor(iso, activity_floor_bq) {
                filtered_below_floor += 1;
                continue;
            }
            for (&ts, &av) in iso.time_grid_s.iter().zip(iso.activity_vs_time_bq.iter()) {
                if ts >= t_irr {
                    sid.push(sim_id.to_string());
                    li.push((idx + 1) as i64);
                    iso_name.push(iso.name.clone());
                    t.push(ts);
                    act.push(av);
                }
            }
        }
    }
    let table = Table {
        name: "cooling",
        cols: vec![
            Col::Str(COOL_SIM_ID, sid),
            Col::I64(COOL_LAYER_INDEX, li),
            Col::Str(COOL_ISOTOPE, iso_name),
            Col::F64(COOL_T_S, t),
            Col::F64(COOL_ACTIVITY, act),
        ],
    };
    FilteredTable {
        table,
        filtered_below_floor,
    }
}

// -- Depth-profile table ---------------------------------------------------

const DEPTH_SIM_ID: ColSpec = INV_SIM_ID;
const DEPTH_LAYER_INDEX: ColSpec = INV_LAYER_INDEX;
const DEPTH_ISOTOPE: ColSpec = INV_ISOTOPE;
const DEPTH_DEPTH_CM: ColSpec = ColSpec::new(
    "depth_cm",
    "cm",
    "Depth into the layer along beam direction (per-row)",
    EvalPoint::PerDepthRow,
);
const DEPTH_ENERGY: ColSpec = ColSpec::new(
    "energy_mev",
    "MeV",
    "Local beam energy at this depth after in-layer degradation",
    EvalPoint::PerDepthRow,
);
const DEPTH_PROD_RATE: ColSpec = ColSpec::new(
    "production_rate_atoms_per_s_per_cm",
    "atoms/s/cm",
    "Local production rate density at this depth (integrate over layer for total /s)",
    EvalPoint::PerDepthRow,
);

/// Depth-profile table: local production rate [atoms/s/cm] along depth, one row
/// per (isotope × layer × depth point). Filtered by `activity_floor_bq` on the
/// owning isotope (per #567) — a depth profile is the *shape* of that isotope's
/// production; if the isotope itself is below the reporting floor its depth
/// series is dropped too.
pub fn build_depth(result: &StackResult, sim_id: &str, activity_floor_bq: f64) -> FilteredTable {
    let (mut sid, mut li, mut iso_name) = (vec![], vec![], vec![]);
    let (mut depth, mut energy, mut prate) = (vec![], vec![], vec![]);
    let mut filtered_below_floor = 0usize;
    for (idx, lr) in result.layer_results.iter().enumerate() {
        for (name, rates) in &lr.depth_production_rates {
            if lr.depth_profile.len() != rates.len() {
                continue; // sibling-array invariant broken — skip rather than zip-truncate
            }
            if activity_floor_bq > 0.0 {
                match lr.isotope_results.get(name) {
                    Some(iso) if !passes_activity_floor(iso, activity_floor_bq) => {
                        filtered_below_floor += 1;
                        continue;
                    }
                    None => continue, // orphan rate series — no owning isotope to filter against
                    _ => {}
                }
            }
            for (dp, &r) in lr.depth_profile.iter().zip(rates.iter()) {
                sid.push(sim_id.to_string());
                li.push((idx + 1) as i64);
                iso_name.push(name.clone());
                depth.push(dp.depth_cm);
                energy.push(dp.energy_mev);
                prate.push(r);
            }
        }
    }
    let table = Table {
        name: "depth",
        cols: vec![
            Col::Str(DEPTH_SIM_ID, sid),
            Col::I64(DEPTH_LAYER_INDEX, li),
            Col::Str(DEPTH_ISOTOPE, iso_name),
            Col::F64(DEPTH_DEPTH_CM, depth),
            Col::F64(DEPTH_ENERGY, energy),
            Col::F64(DEPTH_PROD_RATE, prate),
        ],
    };
    FilteredTable {
        table,
        filtered_below_floor,
    }
}

/// Total activity time series for `isotope`, summed across every layer that
/// produces it (photons leave the whole stack, not one layer). Returns the
/// shared time grid and the elementwise activity sum; layers whose grid length
/// disagrees with the reference are skipped (should not happen within one sim).
fn total_activity(result: &StackResult, isotope: &str) -> (Vec<f64>, Vec<f64>) {
    let mut grid: Vec<f64> = Vec::new();
    let mut total: Vec<f64> = Vec::new();
    for lr in &result.layer_results {
        if let Some(iso) = lr.isotope_results.get(isotope) {
            if grid.is_empty() {
                grid = iso.time_grid_s.clone();
                total = iso.activity_vs_time_bq.clone();
            } else if iso.activity_vs_time_bq.len() == total.len() {
                for (acc, v) in total.iter_mut().zip(iso.activity_vs_time_bq.iter()) {
                    *acc += v;
                }
            }
        }
    }
    (grid, total)
}

/// Best-effort shared time grid — the grid every producing isotope publishes.
/// Used by [`DatasetMeta::time_grid_s`] so a self-contained Parquet describes
/// which sampling times its `t_s` values reference.
pub fn shared_time_grid(result: &StackResult) -> Vec<f64> {
    for lr in &result.layer_results {
        if let Some(iso) = lr.isotope_results.values().next() {
            if !iso.time_grid_s.is_empty() {
                return iso.time_grid_s.clone();
            }
        }
    }
    Vec::new()
}

// -- Emission-rate curve table --------------------------------------------

const EC_SIM_ID: ColSpec = INV_SIM_ID;
const EC_ISOTOPE: ColSpec = INV_ISOTOPE;
const EC_ENERGY: ColSpec = ColSpec::new(
    "energy_kev",
    "keV",
    "Emission-line energy",
    EvalPoint::Static,
);
const EC_EMISSION_TYPE: ColSpec = ColSpec::new(
    "emission_type",
    "",
    "Radiation type: 'gamma' | 'xray' | 'auger' | 'ce' | 'beta-' | 'beta+' | 'annihilation'",
    EvalPoint::Static,
);
const EC_T_S: ColSpec = COOL_T_S;
const EC_RATE: ColSpec = ColSpec::new(
    "rate_per_s",
    "1/s",
    "Emission rate = stack-summed activity at row's t_s × line's intensity_per_decay",
    EvalPoint::PerTimeGridRow,
);

/// Emission-rate curve table (#427): photon/particle emission rate
/// `rate_per_s(t) = total_activity(t) × intensity_per_decay` per (isotope ×
/// line × time point). Filters: `iso_filter` (exact isotope), `type_filter`
/// (rad_type), `energy_filter` (± `energy_tol` keV). `cooling_only` keeps only
/// t ≥ irradiation time.
#[allow(clippy::too_many_arguments)]
pub fn build_emission_curve(
    db: &dyn DatabaseProtocol,
    result: &StackResult,
    sim_id: &str,
    cooling_only: bool,
    iso_filter: Option<&str>,
    type_filter: Option<&str>,
    energy_filter: Option<f64>,
    energy_tol: f64,
    activity_floor_bq: f64,
) -> FilteredTable {
    let t_irr = result.irradiation_time_s;
    let (mut sid, mut iso_name, mut energy, mut etype, mut t, mut rate) =
        (vec![], vec![], vec![], vec![], vec![], vec![]);
    let mut filtered_below_floor = 0usize;

    for (z, a, state, name, total_activity_bq) in distinct_isotopes(result) {
        if iso_filter.is_some_and(|f| f != name) {
            continue;
        }
        // Reporting-layer floor (#567) — filter distinct isotopes by their
        // stack-summed EOC activity, matching the semantics of `compute_stack_dose`.
        if activity_floor_bq > 0.0 && total_activity_bq < activity_floor_bq {
            filtered_below_floor += 1;
            continue;
        }
        let lines: Vec<EmissionLine> = db
            .get_emissions(z, a, &state)
            .into_iter()
            .filter(|l| {
                type_filter.is_none_or(|tf| l.rad_type == tf)
                    && energy_filter.is_none_or(|e| (l.energy_kev - e).abs() <= energy_tol)
            })
            .collect();
        if lines.is_empty() {
            continue;
        }

        let (grid, activity) = total_activity(result, &name);
        if grid.is_empty() {
            continue;
        }

        for line in &lines {
            for (ti, &ts) in grid.iter().enumerate() {
                if cooling_only && ts < t_irr {
                    continue;
                }
                sid.push(sim_id.to_string());
                iso_name.push(name.clone());
                energy.push(line.energy_kev);
                etype.push(line.rad_type.clone());
                t.push(ts);
                rate.push(activity[ti] * line.intensity_per_decay);
            }
        }
    }

    let table = Table {
        name: "emission_curve",
        cols: vec![
            Col::Str(EC_SIM_ID, sid),
            Col::Str(EC_ISOTOPE, iso_name),
            Col::F64(EC_ENERGY, energy),
            Col::Str(EC_EMISSION_TYPE, etype),
            Col::F64(EC_T_S, t),
            Col::F64(EC_RATE, rate),
        ],
    };
    FilteredTable {
        table,
        filtered_below_floor,
    }
}

// -- Emission (per-line) table --------------------------------------------

const EM_SIM_ID: ColSpec = INV_SIM_ID;
const EM_ISOTOPE: ColSpec = INV_ISOTOPE;
const EM_Z: ColSpec = INV_Z;
const EM_A: ColSpec = INV_A;
const EM_STATE: ColSpec = INV_STATE;
const EM_RAD_TYPE: ColSpec = ColSpec::new(
    "rad_type",
    "",
    "Radiation type of this line: 'gamma' | 'xray' | 'auger' | 'ce' | 'beta-' | 'beta+' | 'annihilation'",
    EvalPoint::Static,
);
const EM_ENERGY: ColSpec = EC_ENERGY;
const EM_INTENSITY: ColSpec = ColSpec::new(
    "intensity_per_decay",
    "1/decay",
    "Absolute per-decay intensity (a 511 keV pair reads 2.0; Co-60 1173 keV reads 0.9986)",
    EvalPoint::Static,
);
const EM_DECAY_MODE: ColSpec = ColSpec::nullable(
    "decay_mode",
    "",
    "Feeding decay channel: 'EC' | 'beta-' | 'beta+' | 'IT' | 'KshellEC' / 'LshellEC' / 'MshellEC'",
    EvalPoint::Static,
    "line is present in the ENSDF file but not tagged with a specific feeding decay mode",
);
const EM_DAUGHTER_Z: ColSpec = ColSpec::nullable(
    "daughter_z",
    "",
    "Atomic number of the daughter nuclide fed by this decay channel",
    EvalPoint::Static,
    "no daughter recorded (line has no daughter tag in the ENSDF source)",
);
const EM_DAUGHTER_A: ColSpec = ColSpec::nullable(
    "daughter_a",
    "",
    "Mass number of the daughter nuclide fed by this decay channel",
    EvalPoint::Static,
    "no daughter recorded (line has no daughter tag in the ENSDF source)",
);
const EM_ICC: ColSpec = ColSpec::nullable(
    "icc_total",
    "1",
    "Total internal-conversion coefficient (γ lines only)",
    EvalPoint::Static,
    "no ICC available in the library for this line (usually a non-γ line, or ICC unmeasured — not zero conversion)",
);
const EM_SUBTYPE: ColSpec = ColSpec::nullable(
    "rad_subtype",
    "",
    "Line subtype tag (e.g. 'Kα1' for X-rays, 'KLL' for Augers)",
    EvalPoint::Static,
    "no subtype tag (γ lines and most CE/β± lines have no subtype in the source)",
);

/// Emission table: per-decay γ / X-ray / Auger / CE / β± / annihilation lines
/// for every produced isotope (layer-independent), one row per (isotope × line).
/// `activity_floor_bq` filters distinct isotopes by their stack-summed EOC
/// activity (#567); pass `0.0` for no filtering.
pub fn build_emissions(
    db: &dyn DatabaseProtocol,
    result: &StackResult,
    sim_id: &str,
    activity_floor_bq: f64,
) -> FilteredTable {
    let (mut sid, mut iso_name, mut z, mut a, mut state) = (vec![], vec![], vec![], vec![], vec![]);
    let (mut rad, mut energy, mut intensity) = (vec![], vec![], vec![]);
    let (mut dmode, mut dz, mut da, mut icc, mut subtype) =
        (vec![], vec![], vec![], vec![], vec![]);
    let mut filtered_below_floor = 0usize;

    for (iso_z, iso_a, iso_state, name, total_activity_bq) in distinct_isotopes(result) {
        if activity_floor_bq > 0.0 && total_activity_bq < activity_floor_bq {
            filtered_below_floor += 1;
            continue;
        }
        for line in db.get_emissions(iso_z, iso_a, &iso_state) {
            sid.push(sim_id.to_string());
            iso_name.push(name.clone());
            z.push(iso_z as i64);
            a.push(iso_a as i64);
            state.push(iso_state.clone());
            rad.push(line.rad_type);
            energy.push(line.energy_kev);
            intensity.push(line.intensity_per_decay);
            dmode.push(line.decay_mode);
            dz.push(line.daughter_z.map(|v| v as i64));
            da.push(line.daughter_a.map(|v| v as i64));
            icc.push(line.icc_total);
            subtype.push(line.rad_subtype);
        }
    }

    let table = Table {
        name: "emissions",
        cols: vec![
            Col::Str(EM_SIM_ID, sid),
            Col::Str(EM_ISOTOPE, iso_name),
            Col::I64(EM_Z, z),
            Col::I64(EM_A, a),
            Col::Str(EM_STATE, state),
            Col::Str(EM_RAD_TYPE, rad),
            Col::F64(EM_ENERGY, energy),
            Col::F64(EM_INTENSITY, intensity),
            Col::OptStr(EM_DECAY_MODE, dmode),
            Col::OptI64(EM_DAUGHTER_Z, dz),
            Col::OptI64(EM_DAUGHTER_A, da),
            Col::OptF64(EM_ICC, icc),
            Col::OptStr(EM_SUBTYPE, subtype),
        ],
    };
    FilteredTable {
        table,
        filtered_below_floor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_spec(name: &'static str, unit: &'static str) -> ColSpec {
        ColSpec::new(name, unit, "test column", EvalPoint::Static)
    }

    fn sample() -> Table {
        Table {
            name: "t",
            cols: vec![
                Col::I64(sample_spec("i", ""), vec![1, 2]),
                Col::OptI64(
                    ColSpec::nullable("oi", "", "opt int", EvalPoint::Static, "no value recorded"),
                    vec![Some(7), None],
                ),
                Col::F64(sample_spec("f", "Bq"), vec![1.5, 2.5]),
                Col::OptF64(
                    ColSpec::nullable("of", "s", "opt float", EvalPoint::Static, "unknown"),
                    vec![None, Some(9.0)],
                ),
                Col::Str(sample_spec("s", ""), vec!["a".into(), "b".into()]),
                Col::OptStr(
                    ColSpec::nullable("os", "", "opt str", EvalPoint::Static, "unfiled"),
                    vec![Some("x".into()), None],
                ),
            ],
        }
    }

    #[test]
    fn json_rows_are_long_format_with_nulls() {
        let rows = sample().to_json_rows();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["i"], json!(1));
        assert_eq!(rows[0]["oi"], json!(7));
        assert_eq!(rows[1]["oi"], Value::Null);
        assert_eq!(rows[0]["of"], Value::Null);
        assert_eq!(rows[1]["of"], json!(9.0));
        assert_eq!(rows[0]["s"], json!("a"));
        assert_eq!(rows[1]["os"], Value::Null);
    }

    #[test]
    fn schema_json_carries_unit_description_eval_point_and_null_meaning() {
        let s = sample().schema_json();
        let arr = s.as_array().expect("schema_json is an array");
        assert_eq!(arr.len(), 6);
        // Non-nullable spec: nullable=false, no null_meaning key.
        let i = &arr[0];
        assert_eq!(i["name"], "i");
        assert_eq!(i["unit"], "");
        assert_eq!(i["eval_point"], "static");
        assert_eq!(i["nullable"], false);
        assert!(i.get("null_meaning").is_none());
        // Nullable spec: nullable=true, null_meaning populated.
        let oi = &arr[1];
        assert_eq!(oi["nullable"], true);
        assert_eq!(oi["null_meaning"], "no value recorded");
        // Unit rides through.
        let f = &arr[2];
        assert_eq!(f["unit"], "Bq");
    }

    #[test]
    fn empty_table_reports_empty() {
        let t = Table {
            name: "e",
            cols: vec![Col::I64(sample_spec("i", ""), vec![])],
        };
        assert!(t.is_empty());
        assert_eq!(t.to_json_rows().len(), 0);
        // Still has a schema — empty tables must be self-describing too so a
        // reader can plan a downstream query without waiting for rows.
        assert_eq!(t.schema_json().as_array().unwrap().len(), 1);
    }

    #[test]
    fn parquet_round_trips_through_a_reader_with_field_and_file_metadata() {
        use arrow::array::{Array, Float64Array, Int64Array, StringArray};
        use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
        use std::io::Write;

        let meta = DatasetMeta {
            simulation_id: "sim-abc".into(),
            core_version: "9.9.9",
            library: "tendl-2023-iso".into(),
            config_json: r#"{"projectile":"p","energy_mev":18.0}"#.into(),
            time_grid_s: vec![0.0, 3600.0, 7200.0],
            irradiation_time_s: 3600.0,
            cooling_time_s: 3600.0,
            table_name: "t".into(),
        };
        let bytes = sample().to_parquet_bytes(Some(&meta)).unwrap();
        assert_eq!(&bytes[..4], b"PAR1");
        assert_eq!(&bytes[bytes.len() - 4..], b"PAR1");

        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(&bytes).unwrap();
        let file = tmp.reopen().unwrap();
        let builder = ParquetRecordBatchReaderBuilder::try_new(file).unwrap();

        // File-level Parquet KV metadata carries the dataset provenance —
        // visible to any Parquet reader, not just Arrow-aware ones.
        let file_meta = builder.metadata().file_metadata();
        let kv = file_meta
            .key_value_metadata()
            .expect("dataset metadata must be attached to the Parquet file KV");
        let get = |k: &str| {
            kv.iter()
                .find(|e| e.key == k)
                .and_then(|e| e.value.clone())
                .unwrap_or_else(|| panic!("Parquet file KV missing '{k}'"))
        };
        assert_eq!(get("hyrr.simulation_id"), "sim-abc");
        assert_eq!(get("hyrr.core_version"), "9.9.9");
        assert_eq!(get("hyrr.library"), "tendl-2023-iso");
        assert_eq!(get("hyrr.table_name"), "t");
        assert_eq!(get("hyrr.irradiation_time_s"), "3600");
        assert!(get("hyrr.config_json").contains("\"projectile\":\"p\""));
        // Time grid JSON-encoded, decodes back cleanly.
        let grid: Vec<f64> = serde_json::from_str(&get("hyrr.time_grid_s_json")).unwrap();
        assert_eq!(grid, vec![0.0, 3600.0, 7200.0]);

        // Field-level Arrow metadata round-trips (unit + eval_point + null_meaning).
        let arrow_schema = builder.schema();
        let f_field = arrow_schema.field_with_name("f").unwrap();
        assert_eq!(
            f_field.metadata().get("hyrr.unit").map(String::as_str),
            Some("Bq")
        );
        assert_eq!(
            f_field
                .metadata()
                .get("hyrr.eval_point")
                .map(String::as_str),
            Some("static")
        );
        let of_field = arrow_schema.field_with_name("of").unwrap();
        assert!(of_field.metadata().get("hyrr.null_meaning").is_some());

        // Data still round-trips.
        let mut reader = builder.build().unwrap();
        let batch = reader.next().unwrap().unwrap();
        assert_eq!(batch.num_rows(), 2);
        let col = |name: &str| batch.column(batch.schema().index_of(name).unwrap()).clone();
        let i = col("i");
        let i = i.as_any().downcast_ref::<Int64Array>().unwrap();
        assert_eq!(i.value(0), 1);
        let oi = col("oi");
        let oi = oi.as_any().downcast_ref::<Int64Array>().unwrap();
        assert_eq!(oi.value(0), 7);
        assert!(oi.is_null(1));
        let f = col("f");
        let f = f.as_any().downcast_ref::<Float64Array>().unwrap();
        assert!((f.value(1) - 2.5).abs() < 1e-12);
        let s = col("s");
        let s = s.as_any().downcast_ref::<StringArray>().unwrap();
        assert_eq!(s.value(0), "a");
        let os = col("os");
        let os = os.as_any().downcast_ref::<StringArray>().unwrap();
        assert_eq!(os.value(0), "x");
        assert!(os.is_null(1));
    }

    #[test]
    fn parquet_without_dataset_meta_still_writes() {
        // A table can be serialized without provenance (unit test / debug
        // path). Field-level metadata still ships.
        let bytes = sample().to_parquet_bytes(None).unwrap();
        assert_eq!(&bytes[..4], b"PAR1");
    }

    #[test]
    fn inline_row_order_sorts_descending_by_numeric_column() {
        let t = Table {
            name: "t",
            cols: vec![
                Col::Str(
                    sample_spec("iso", ""),
                    vec!["a".into(), "b".into(), "c".into()],
                ),
                Col::F64(sample_spec("v", "Bq"), vec![1.0, 5.0, 3.0]),
            ],
        };
        let order = t.inline_row_order(Some("v")).unwrap();
        assert_eq!(order, vec![1, 2, 0], "descending by v: 5, 3, 1");
    }

    #[test]
    fn inline_row_order_puts_nulls_last() {
        let t = Table {
            name: "t",
            cols: vec![
                Col::Str(
                    sample_spec("iso", ""),
                    vec!["a".into(), "b".into(), "c".into()],
                ),
                Col::OptF64(
                    ColSpec::nullable("v", "Bq", "d", EvalPoint::Static, "n/a"),
                    vec![Some(1.0), None, Some(3.0)],
                ),
            ],
        };
        let order = t.inline_row_order(Some("v")).unwrap();
        assert_eq!(order[0], 2, "3.0 first");
        assert_eq!(order[1], 0, "1.0 second");
        assert_eq!(order[2], 1, "None last (not dropped)");
    }

    #[test]
    fn inline_row_order_rejects_unknown_or_non_numeric_sort_key() {
        let t = Table {
            name: "t",
            cols: vec![
                Col::Str(sample_spec("s", ""), vec!["a".into()]),
                Col::F64(sample_spec("v", "Bq"), vec![1.0]),
            ],
        };
        let err = t.inline_row_order(Some("nope")).unwrap_err();
        assert!(err.contains("unknown column 'nope'"), "got: {err}");
        let err = t.inline_row_order(Some("s")).unwrap_err();
        assert!(err.contains("not numeric"), "got: {err}");
    }

    #[test]
    fn inline_json_rows_truncates_but_reports_total() {
        let t = Table {
            name: "t",
            cols: vec![Col::F64(sample_spec("v", "Bq"), vec![1.0, 2.0, 3.0, 4.0])],
        };
        let (rows, trunc) = t.inline_json_rows(Some(2), Some("v")).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(trunc, Some(4), "must report the total when truncated");
        // Descending by v → 4.0, 3.0.
        assert_eq!(rows[0]["v"], json!(4.0));
        assert_eq!(rows[1]["v"], json!(3.0));

        // No cap → no truncation flag.
        let (rows, trunc) = t.inline_json_rows(None, None).unwrap();
        assert_eq!(rows.len(), 4);
        assert_eq!(trunc, None);

        // Cap ≥ total → no truncation flag.
        let (rows, trunc) = t.inline_json_rows(Some(99), None).unwrap();
        assert_eq!(rows.len(), 4);
        assert_eq!(trunc, None);
    }
}
