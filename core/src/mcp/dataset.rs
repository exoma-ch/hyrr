//! Structured-export tables for the #427 dataset tools.
//!
//! A [`Table`] is a set of named, typed columns in long format. The same
//! column source feeds both the inline JSON (`to_json_rows`) and the embedded
//! Parquet resource (`to_parquet_bytes`), so the two representations can never
//! drift. Builders turn a [`StackResult`] (+ decay/emission data) into the
//! inventory / cooling-tail / depth-profile / emission tables the issue spells
//! out.

use serde_json::{Map, Number, Value};

use crate::db::DatabaseProtocol;
use crate::types::{EmissionLine, IsotopeResult, StackResult};

/// One typed column. `&'static str` names keep the schema literal at the call
/// site; nullable variants map to Parquet nullable + JSON `null`.
pub enum Col {
    I64(&'static str, Vec<i64>),
    OptI64(&'static str, Vec<Option<i64>>),
    F64(&'static str, Vec<f64>),
    OptF64(&'static str, Vec<Option<f64>>),
    Str(&'static str, Vec<String>),
    OptStr(&'static str, Vec<Option<String>>),
}

impl Col {
    fn name(&self) -> &'static str {
        match self {
            Col::I64(n, _)
            | Col::OptI64(n, _)
            | Col::F64(n, _)
            | Col::OptF64(n, _)
            | Col::Str(n, _)
            | Col::OptStr(n, _) => n,
        }
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

    /// Long-format rows as JSON objects (one per row).
    pub fn to_json_rows(&self) -> Vec<Value> {
        (0..self.nrows())
            .map(|i| {
                let mut obj = Map::new();
                for col in &self.cols {
                    obj.insert(col.name().to_string(), col.json_at(i));
                }
                Value::Object(obj)
            })
            .collect()
    }

    /// Serialize to in-memory Parquet bytes via Arrow.
    pub fn to_parquet_bytes(&self) -> Result<Vec<u8>, String> {
        use arrow::array::{ArrayRef, Float64Array, Int64Array, StringArray};
        use arrow::datatypes::{DataType, Field, Schema};
        use arrow::record_batch::RecordBatch;
        use parquet::arrow::ArrowWriter;
        use std::sync::Arc;

        let mut fields = Vec::with_capacity(self.cols.len());
        let mut arrays: Vec<ArrayRef> = Vec::with_capacity(self.cols.len());
        for col in &self.cols {
            let (field, array): (Field, ArrayRef) = match col {
                Col::I64(n, v) => (
                    Field::new(*n, DataType::Int64, false),
                    Arc::new(Int64Array::from(v.clone())),
                ),
                Col::OptI64(n, v) => (
                    Field::new(*n, DataType::Int64, true),
                    Arc::new(Int64Array::from(v.clone())),
                ),
                Col::F64(n, v) => (
                    Field::new(*n, DataType::Float64, false),
                    Arc::new(Float64Array::from(v.clone())),
                ),
                Col::OptF64(n, v) => (
                    Field::new(*n, DataType::Float64, true),
                    Arc::new(Float64Array::from(v.clone())),
                ),
                Col::Str(n, v) => (
                    Field::new(*n, DataType::Utf8, false),
                    Arc::new(StringArray::from_iter(v.iter().map(|s| Some(s.as_str())))),
                ),
                Col::OptStr(n, v) => (
                    Field::new(*n, DataType::Utf8, true),
                    Arc::new(StringArray::from_iter(v.iter().map(|o| o.as_deref()))),
                ),
            };
            fields.push(field);
            arrays.push(array);
        }

        let schema = Arc::new(Schema::new(fields));
        let batch = RecordBatch::try_new(Arc::clone(&schema), arrays).map_err(|e| e.to_string())?;
        let mut buf = Vec::new();
        {
            let mut writer =
                ArrowWriter::try_new(&mut buf, schema, None).map_err(|e| e.to_string())?;
            writer.write(&batch).map_err(|e| e.to_string())?;
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
/// as `(z, a, state, name)`. Used by the (layer-independent) emission table.
fn distinct_isotopes(result: &StackResult) -> Vec<(u32, u32, String, String)> {
    let mut seen = std::collections::BTreeMap::new();
    for lr in &result.layer_results {
        for iso in lr.isotope_results.values() {
            seen.entry((iso.z, iso.a, iso.state.clone()))
                .or_insert_with(|| iso.name.clone());
        }
    }
    seen.into_iter()
        .map(|((z, a, st), name)| (z, a, st, name))
        .collect()
}

/// Inventory table: one row per (isotope × layer × source).
pub fn build_inventory(
    db: &dyn DatabaseProtocol,
    result: &StackResult,
    layer_materials: &[String],
    sim_id: &str,
) -> Table {
    let t_irr = result.irradiation_time_s;
    let (mut sid, mut li, mut mat) = (vec![], vec![], vec![]);
    let (mut z, mut a, mut state, mut iso_name) = (vec![], vec![], vec![], vec![]);
    let (mut src, mut rate, mut sat) = (vec![], vec![], vec![]);
    let (mut eob, mut cool, mut hl) = (vec![], vec![], vec![]);
    let (mut bp, mut ec, mut bm, mut it) = (vec![], vec![], vec![], vec![]);

    for (idx, lr) in result.layer_results.iter().enumerate() {
        let mut isos: Vec<&IsotopeResult> = lr.isotope_results.values().collect();
        isos.sort_by(|x, y| {
            y.activity_bq
                .partial_cmp(&x.activity_bq)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| x.name.cmp(&y.name))
        });
        for iso in isos {
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

    Table {
        name: "inventory",
        cols: vec![
            Col::Str("simulation_id", sid),
            Col::I64("layer_index", li),
            Col::Str("layer_material", mat),
            Col::I64("z", z),
            Col::I64("a", a),
            Col::Str("state", state),
            Col::Str("isotope", iso_name),
            Col::Str("production_source", src),
            Col::F64("production_rate_per_s", rate),
            Col::F64("saturation_yield_bq_per_ua", sat),
            Col::F64("activity_at_eob_bq", eob),
            Col::F64("activity_at_cooling_bq", cool),
            Col::OptF64("half_life_s", hl),
            Col::F64("beta_plus_branching", bp),
            Col::F64("ec_branching", ec),
            Col::F64("beta_minus_branching", bm),
            Col::F64("it_branching", it),
        ],
    }
}

/// Cooling-tail table: activity [Bq] vs time for t ≥ irradiation time, one row
/// per (isotope × layer × time point).
pub fn build_cooling(result: &StackResult, sim_id: &str) -> Table {
    let t_irr = result.irradiation_time_s;
    let (mut sid, mut li, mut iso_name, mut t, mut act) = (vec![], vec![], vec![], vec![], vec![]);
    for (idx, lr) in result.layer_results.iter().enumerate() {
        for iso in lr.isotope_results.values() {
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
    Table {
        name: "cooling",
        cols: vec![
            Col::Str("simulation_id", sid),
            Col::I64("layer_index", li),
            Col::Str("isotope", iso_name),
            Col::F64("t_s", t),
            Col::F64("activity_bq", act),
        ],
    }
}

/// Depth-profile table: local production rate [atoms/s/cm] along depth, one row
/// per (isotope × layer × depth point).
pub fn build_depth(result: &StackResult, sim_id: &str) -> Table {
    let (mut sid, mut li, mut iso_name) = (vec![], vec![], vec![]);
    let (mut depth, mut energy, mut prate) = (vec![], vec![], vec![]);
    for (idx, lr) in result.layer_results.iter().enumerate() {
        for (name, rates) in &lr.depth_production_rates {
            if lr.depth_profile.len() != rates.len() {
                continue; // sibling-array invariant broken — skip rather than zip-truncate
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
    Table {
        name: "depth",
        cols: vec![
            Col::Str("simulation_id", sid),
            Col::I64("layer_index", li),
            Col::Str("isotope", iso_name),
            Col::F64("depth_cm", depth),
            Col::F64("energy_mev", energy),
            Col::F64("production_rate_atoms_per_s_per_cm", prate),
        ],
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
) -> Table {
    let t_irr = result.irradiation_time_s;
    let (mut sid, mut iso_name, mut energy, mut etype, mut t, mut rate) =
        (vec![], vec![], vec![], vec![], vec![], vec![]);

    for (z, a, state, name) in distinct_isotopes(result) {
        if iso_filter.is_some_and(|f| f != name) {
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

    Table {
        name: "emission_curve",
        cols: vec![
            Col::Str("simulation_id", sid),
            Col::Str("isotope", iso_name),
            Col::F64("energy_kev", energy),
            Col::Str("emission_type", etype),
            Col::F64("t_s", t),
            Col::F64("rate_per_s", rate),
        ],
    }
}

/// Emission table: per-decay γ / X-ray / Auger / CE / β± / annihilation lines
/// for every produced isotope (layer-independent), one row per (isotope × line).
pub fn build_emissions(db: &dyn DatabaseProtocol, result: &StackResult, sim_id: &str) -> Table {
    let (mut sid, mut iso_name, mut z, mut a, mut state) = (vec![], vec![], vec![], vec![], vec![]);
    let (mut rad, mut energy, mut intensity) = (vec![], vec![], vec![]);
    let (mut dmode, mut dz, mut da, mut icc, mut subtype) =
        (vec![], vec![], vec![], vec![], vec![]);

    for (iso_z, iso_a, iso_state, name) in distinct_isotopes(result) {
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

    Table {
        name: "emissions",
        cols: vec![
            Col::Str("simulation_id", sid),
            Col::Str("isotope", iso_name),
            Col::I64("z", z),
            Col::I64("a", a),
            Col::Str("state", state),
            Col::Str("rad_type", rad),
            Col::F64("energy_kev", energy),
            Col::F64("intensity_per_decay", intensity),
            Col::OptStr("decay_mode", dmode),
            Col::OptI64("daughter_z", dz),
            Col::OptI64("daughter_a", da),
            Col::OptF64("icc_total", icc),
            Col::OptStr("rad_subtype", subtype),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Table {
        Table {
            name: "t",
            cols: vec![
                Col::I64("i", vec![1, 2]),
                Col::OptI64("oi", vec![Some(7), None]),
                Col::F64("f", vec![1.5, 2.5]),
                Col::OptF64("of", vec![None, Some(9.0)]),
                Col::Str("s", vec!["a".into(), "b".into()]),
                Col::OptStr("os", vec![Some("x".into()), None]),
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
    fn empty_table_reports_empty() {
        let t = Table {
            name: "e",
            cols: vec![Col::I64("i", vec![])],
        };
        assert!(t.is_empty());
        assert_eq!(t.to_json_rows().len(), 0);
    }

    #[test]
    fn parquet_round_trips_through_a_reader() {
        use arrow::array::{Array, Float64Array, Int64Array, StringArray};
        use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
        use std::io::Write;

        let bytes = sample().to_parquet_bytes().unwrap();
        // Structurally a Parquet container.
        assert_eq!(&bytes[..4], b"PAR1");
        assert_eq!(&bytes[bytes.len() - 4..], b"PAR1");

        // Read it back with a real Parquet reader and check the values survive.
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(&bytes).unwrap();
        let file = tmp.reopen().unwrap();
        let mut reader = ParquetRecordBatchReaderBuilder::try_new(file)
            .unwrap()
            .build()
            .unwrap();
        let batch = reader.next().unwrap().unwrap();
        assert_eq!(batch.num_rows(), 2);

        let col = |name: &str| batch.column(batch.schema().index_of(name).unwrap()).clone();
        let i = col("i");
        let i = i.as_any().downcast_ref::<Int64Array>().unwrap();
        assert_eq!(i.value(0), 1);
        assert_eq!(i.value(1), 2);

        let oi = col("oi");
        let oi = oi.as_any().downcast_ref::<Int64Array>().unwrap();
        assert_eq!(oi.value(0), 7);
        assert!(
            oi.is_null(1),
            "OptI64 None must round-trip to a Parquet null"
        );

        let f = col("f");
        let f = f.as_any().downcast_ref::<Float64Array>().unwrap();
        assert!((f.value(1) - 2.5).abs() < 1e-12);

        let s = col("s");
        let s = s.as_any().downcast_ref::<StringArray>().unwrap();
        assert_eq!(s.value(0), "a");

        let os = col("os");
        let os = os.as_any().downcast_ref::<StringArray>().unwrap();
        assert_eq!(os.value(0), "x");
        assert!(
            os.is_null(1),
            "OptStr None must round-trip to a Parquet null"
        );
    }
}
