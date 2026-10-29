05 weather-tooltip orphans (RESOLVED, batch 13)

   render_tooltip_popup and view_popup_content were dead code in
   bar/weather/tooltip.rs. Both removed along with their orphaned
   /// doc blocks in the batch-13 fixup.

   App::view() uses build_content internally; no external callers.
