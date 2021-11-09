/**
 * This file will contains tips data for status-bar.
 * To display a tip, data must be created with a `LinePart` structure.
 * 
 * So, can write pseudo-code like this:
 * ```
 * const TIPS_DATA = [
 *   (TIP_ID, [(STRING, COLOR), (..., ...)]),
 *   ...
 * ]
 * 
 * fn get_tips_data(tips_id: &str) -> Option<&[LinePart]> {
 *   ...
 * }
 * ```
 */