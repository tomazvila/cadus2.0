import type { RenderedVisual } from '@/lib/visual';

/** A figure as the service draws it for a teach page: `render_all` output for a 4/6 fraction bar. */
export const DEMO_FIGURE: RenderedVisual = {
  kind: 'fraction',
  svg:
    "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 480 200\" class=\"cadus-visual cadus-visual-fraction\" role=\"img\" aria-labelledby=\"teach-0-title teach-0-desc\"><title id=\"teach-0-title\">A bar of 6 equal parts with 4 shaded.</title><desc id=\"teach-0-desc\">A bar of 6 equal parts with 4 shaded. A fraction bar divided into 6 equal parts, with 4 of them shaded. The figure shows 4/6.</desc><rect x=\"36.00\" y=\"36.00\" width=\"68.00\" height=\"90.00\" class=\"cadus-visual-part-shaded\"/><rect x=\"104.00\" y=\"36.00\" width=\"68.00\" height=\"90.00\" class=\"cadus-visual-part-shaded\"/><rect x=\"172.00\" y=\"36.00\" width=\"68.00\" height=\"90.00\" class=\"cadus-visual-part-shaded\"/><rect x=\"240.00\" y=\"36.00\" width=\"68.00\" height=\"90.00\" class=\"cadus-visual-part-shaded\"/><rect x=\"308.00\" y=\"36.00\" width=\"68.00\" height=\"90.00\" class=\"cadus-visual-part\"/><rect x=\"376.00\" y=\"36.00\" width=\"68.00\" height=\"90.00\" class=\"cadus-visual-part\"/></svg>",
  text: "A bar of 6 equal parts with 4 shaded. A fraction bar divided into 6 equal parts, with 4 of them shaded. The figure shows 4/6.",
};
