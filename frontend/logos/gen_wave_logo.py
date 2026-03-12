"""
Generate HYRR logo: isometric 3D wave-style plot.

Beam axis, Bragg peak (dE/dx) vertical, cross-section (σ) curves perpendicular,
material layer bars opposite. No grid, no axes, transparent background.

Outputs: dark, light, and transparent variants + favicon sizes.
"""

import numpy as np
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from mpl_toolkits.mplot3d.art3d import Poly3DCollection

LAYER_COLORS = ["#2dd4bf", "#4ade80", "#fbbf24", "#d97706", "#8b949e"]

# ── Physics data ────────────────────────────────────────────
z = np.linspace(0, 10, 300)

def bragg_curve(z, z_peak=8.2, width=0.6):
    y = np.zeros_like(z)
    mask_rise = z <= z_peak
    y[mask_rise] = 0.15 + 0.85 * (z[mask_rise] / z_peak) ** 2.5
    mask_fall = z > z_peak
    y[mask_fall] = np.exp(-((z[mask_fall] - z_peak) / width) ** 2)
    return y

bragg = bragg_curve(z) * 4.0

def excitation_fn(z, threshold, peak_z, width, amplitude):
    y = np.zeros_like(z)
    mask = z >= threshold
    x = z[mask] - threshold
    peak_x = peak_z - threshold
    if peak_x <= 0:
        peak_x = 0.5
    sigma_left = peak_x * 0.6
    sigma_right = peak_x * 2.5
    sigma = np.where(x < peak_x, sigma_left, sigma_right)
    y[mask] = amplitude * np.exp(-0.5 * ((x - peak_x) / sigma) ** 2)
    cutoff_start = 6.0
    cutoff_end = 7.5
    fade = np.clip((cutoff_end - z) / (cutoff_end - cutoff_start), 0, 1)
    y *= fade
    y[y < 0] = 0
    return y

xs_channels = [
    {"threshold": 0.0, "peak_z": 2.0, "amplitude": 3.5, "color": "#2dd4bf"},
    {"threshold": 0.0, "peak_z": 1.2, "amplitude": 2.8, "color": "#4ade80"},
    {"threshold": 0.0, "peak_z": 0.8, "amplitude": 2.0, "color": "#fbbf24"},
    {"threshold": 0.0, "peak_z": 0.5, "amplitude": 1.4, "color": "#a78bfa"},
]

layer_bounds = [0, 2.0, 4.5, 7.0, 8.5, 10.0]
bar_heights = [2.0, 3.2, 2.5, 1.5, 0.8]


def render_logo(bg_color, beam_color, bragg_alpha, xs_alpha, bar_alpha,
                edge_curve_alpha, bragg_line_alpha, beam_alpha):
    """Render the 3D logo with given color/alpha settings."""
    fig = plt.figure(figsize=(8, 8), dpi=128)
    fig.patch.set_facecolor(bg_color)
    fig.patch.set_alpha(0.0 if bg_color == "none" else 1.0)

    ax = fig.add_subplot(111, projection="3d", computed_zorder=False)
    ax.set_facecolor("none" if bg_color == "none" else bg_color)
    ax.view_init(elev=25, azim=-55)

    # ── Material bars ──
    for i in range(len(layer_bounds) - 1):
        y0, y1 = layer_bounds[i], layer_bounds[i + 1]
        h = bar_heights[i]
        color = LAYER_COLORS[i]
        gap = 0.08

        verts_bar = [[(0, y0+gap, 0), (0, y1-gap, 0), (h, y1-gap, 0), (h, y0+gap, 0)]]
        ax.add_collection3d(Poly3DCollection(verts_bar, alpha=bar_alpha,
                            facecolor=color, edgecolor=color, linewidth=0.4))

        verts_edge = [[(0, y1-gap, -0.15), (h, y1-gap, -0.15), (h, y1-gap, 0), (0, y1-gap, 0)]]
        ax.add_collection3d(Poly3DCollection(verts_edge, alpha=bar_alpha*0.55,
                            facecolor=color, edgecolor="none", linewidth=0))

        verts_right = [[(h, y0+gap, -0.15), (h, y1-gap, -0.15), (h, y1-gap, 0), (h, y0+gap, 0)]]
        ax.add_collection3d(Poly3DCollection(verts_right, alpha=bar_alpha*0.45,
                            facecolor=color, edgecolor="none", linewidth=0))

    # ── Bragg peak ──
    verts_bragg = []
    for i in range(len(z) - 1):
        verts_bragg.append([
            (0, z[i], 0), (0, z[i+1], 0),
            (0, z[i+1], bragg[i+1]), (0, z[i], bragg[i]),
        ])
    ax.add_collection3d(Poly3DCollection(verts_bragg, alpha=bragg_alpha,
                        facecolor="#ef4444", edgecolor="none", linewidth=0))
    ax.plot(np.zeros_like(z), z, bragg, color="#ef4444",
            linewidth=1.2, alpha=bragg_line_alpha, zorder=5)

    # ── XS curves ──
    for ch in xs_channels:
        xs_vals = excitation_fn(z, ch["threshold"], ch["peak_z"], 1.5, ch["amplitude"])

        verts_xs = []
        for i in range(len(z) - 1):
            if xs_vals[i] > 0.01 or xs_vals[i+1] > 0.01:
                verts_xs.append([
                    (0, z[i], 0), (0, z[i+1], 0),
                    (-xs_vals[i+1], z[i+1], 0), (-xs_vals[i], z[i], 0),
                ])
        if verts_xs:
            ax.add_collection3d(Poly3DCollection(verts_xs, alpha=xs_alpha,
                                facecolor=ch["color"], edgecolor="none", linewidth=0))
        ax.plot(-xs_vals, z, np.zeros_like(z), color=ch["color"],
                linewidth=1.0, alpha=edge_curve_alpha, zorder=4)

    # ── Beam axis ──
    ax.plot([0, 0], [-0.5, 10.5], [0, 0], color=beam_color,
            linewidth=2.0, alpha=beam_alpha, zorder=10)

    # ── Remove ALL axes, grid, panes, ticks, labels ──
    ax.set_axis_off()

    # ── Tighter limits to fill the frame ──
    ax.set_xlim(-4.0, 3.8)
    ax.set_ylim(-0.5, 10.8)
    ax.set_zlim(-0.15, 4.8)

    fig.subplots_adjust(left=-0.15, right=1.15, bottom=-0.1, top=1.1)

    return fig


# ── Generate variants ───────────────────────────────────────
OUT = "/Users/larsgerchow/Projects/eXoma/hyrr/frontend/logos/"
PUBLIC = "/Users/larsgerchow/Projects/eXoma/hyrr/frontend/public/"

# Dark version (dark bg)
fig = render_logo(
    bg_color="#0d1117", beam_color="#58a6ff",
    bragg_alpha=0.35, xs_alpha=0.30, bar_alpha=0.70,
    edge_curve_alpha=0.80, bragg_line_alpha=0.75, beam_alpha=0.9,
)
fig.savefig(OUT + "hyrr-logo-dark.svg", format="svg", facecolor="#0d1117",
            bbox_inches="tight", pad_inches=0)
fig.savefig(OUT + "hyrr-logo-dark.png", format="png", facecolor="#0d1117",
            dpi=256, bbox_inches="tight", pad_inches=0)
plt.close(fig)

# Light version (white bg, darker colors)
fig = render_logo(
    bg_color="#ffffff", beam_color="#0969da",
    bragg_alpha=0.22, xs_alpha=0.18, bar_alpha=0.50,
    edge_curve_alpha=0.65, bragg_line_alpha=0.6, beam_alpha=0.9,
)
fig.savefig(OUT + "hyrr-logo-light.svg", format="svg", facecolor="#ffffff",
            bbox_inches="tight", pad_inches=0)
fig.savefig(OUT + "hyrr-logo-light.png", format="png", facecolor="#ffffff",
            dpi=256, bbox_inches="tight", pad_inches=0)
plt.close(fig)

# Transparent version
fig = render_logo(
    bg_color="none", beam_color="#58a6ff",
    bragg_alpha=0.35, xs_alpha=0.30, bar_alpha=0.70,
    edge_curve_alpha=0.80, bragg_line_alpha=0.75, beam_alpha=0.9,
)
fig.savefig(OUT + "hyrr-logo-transparent.svg", format="svg",
            facecolor="none", transparent=True, bbox_inches="tight", pad_inches=0)
fig.savefig(OUT + "hyrr-logo-transparent.png", format="png",
            facecolor="none", transparent=True, dpi=256,
            bbox_inches="tight", pad_inches=0)
plt.close(fig)

# Favicon sizes (from dark version)
for size in [32, 180, 192, 512]:
    fig = render_logo(
        bg_color="none" if size <= 32 else "#0d1117",
        beam_color="#58a6ff",
        bragg_alpha=0.35, xs_alpha=0.30, bar_alpha=0.70,
        edge_curve_alpha=0.80, bragg_line_alpha=0.75, beam_alpha=0.9,
    )
    fig.savefig(OUT + f"hyrr-icon-{size}.png", format="png",
                facecolor="none" if size <= 32 else "#0d1117",
                transparent=(size <= 32), dpi=size / 8,
                bbox_inches="tight", pad_inches=0)
    # Also save to public for web use
    fig.savefig(PUBLIC + f"hyrr-icon-{size}.png", format="png",
                facecolor="none" if size <= 32 else "#0d1117",
                transparent=(size <= 32), dpi=size / 8,
                bbox_inches="tight", pad_inches=0)
    plt.close(fig)

# Copy to public for web use
import shutil
shutil.copy(OUT + "hyrr-logo-dark.svg", PUBLIC + "logo.svg")
shutil.copy(OUT + "hyrr-logo-dark.svg", PUBLIC + "favicon.svg")

print("Done — all variants generated")
