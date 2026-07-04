// Shared ECharts foundation (Plan F/F1). One theme-token reader and one lifecycle
// helper so every chart on the site reads the same medieval tokens and disposes
// cleanly across ClientRouter navigations — extracted verbatim from MapCivRankings
// so nothing renders differently; later charts (civ, unit, analyzer) build on it.
//
// Tree-shaking: only the CanvasRenderer (universal) is registered here. Each chart
// still calls `echarts.use([...])` for its own chart type + components, so unused
// pieces stay out of the bundle.
import * as echarts from "echarts/core";
import { CanvasRenderer } from "echarts/renderers";

echarts.use([CanvasRenderer]);

export type { ECharts } from "echarts/core";
export { echarts };

export type ThemeColors = {
  good: string;
  bad: string;
  ink: string;
  muted: string;
  surface: string;
  gold: string;
};

/** Read the medieval theme tokens off :root — they flip on [data-theme]. */
export function themeColors(): ThemeColors {
  const css = getComputedStyle(document.documentElement);
  const v = (name: string, fb: string) => css.getPropertyValue(name).trim() || fb;
  return {
    good: v("--color-wr-good", "#8a5f18"),
    bad: v("--color-wr-bad", "#942124"),
    ink: v("--color-ink", "#221811"),
    muted: v("--color-stone-700", "#41352f"),
    surface: v("--color-parchment", "#f6e6cb"),
    gold: v("--color-gold-500", "#c79a3c"),
  };
}

/**
 * Init an ECharts instance on `el` and wire the two observers every chart needs:
 * a ResizeObserver (responsive) and a [data-theme] MutationObserver (repaint on
 * light/dark swap). Returns the chart plus a dispose() for astro:before-swap.
 */
export function mountChart(el: HTMLElement, opts: { onThemeChange?: () => void } = {}) {
  const chart = echarts.init(el, null, { renderer: "canvas" });
  const resizeObs = new ResizeObserver(() => chart.resize());
  resizeObs.observe(el);
  let themeObs: MutationObserver | null = null;
  if (opts.onThemeChange) {
    themeObs = new MutationObserver(() => opts.onThemeChange?.());
    themeObs.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme"],
    });
  }
  const dispose = () => {
    resizeObs.disconnect();
    themeObs?.disconnect();
    chart.dispose();
  };
  return { chart, dispose };
}
