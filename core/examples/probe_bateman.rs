//! Direct test of bateman_activity — does it emit the correct Bateman curve?
//! If yes, the problem is downstream (clamp, chain solver, etc.)

use hyrr_core::bateman::bateman_activity;

fn main() {
    let r = 1.294e11_f64;
    let hl_s = 122.24_f64; // O-15
    let irr = 7200.0_f64; // 2h
    let cool = 3600.0_f64; // 1h

    let result = bateman_activity(r, Some(hl_s), irr, cool, 200);

    println!("bateman_activity(R={:.3e}, t½={}, irr={}, cool={})", r, hl_s, irr, cool);
    println!(
        "  saturation R × (1 - exp(-λ×irr)) = {:.3e}",
        r * (1.0 - (-std::f64::consts::LN_2 / hl_s * irr).exp())
    );
    println!("  time_grid length: {}", result.time_grid.len());

    // Sample at key points
    let n = result.time_grid.len();
    for &i in &[0, 1, 2, 5, 10, 50, 99, 100, 101, 150, 199] {
        if i >= n {
            continue;
        }
        let t = result.time_grid[i];
        let a = result.activity[i];
        let expected = if t <= irr {
            r * (1.0 - (-std::f64::consts::LN_2 / hl_s * t).exp())
        } else {
            let a_eoi = r * (1.0 - (-std::f64::consts::LN_2 / hl_s * irr).exp());
            a_eoi * (-std::f64::consts::LN_2 / hl_s * (t - irr)).exp()
        };
        let err = if expected > 0.0 { (a - expected).abs() / expected } else { 0.0 };
        println!(
            "  i={:>3}  t={:>8.2}s  A={:.3e}  expected={:.3e}  rel_err={:.2e}",
            i, t, a, expected, err
        );
    }
}
