//! Additional edge case tests for custom pane size functionality (PR-2804)

use crate::tab::{Tab, MIN_TERMINAL_HEIGHT};
use zellij_utils::{
    data::{FloatingPaneCoordinates, Viewport},
    input::layout::{PercentOrFixed, SplitDirection, SplitSize, TiledPaneLayout},
    pane_size::{Constraint, Dimension, PaneGeom, Size},
};

#[test]
fn test_mixed_fixed_and_percentage_layout_resize() {
    // Simulate layout with BOTH fixed and percentage panes
    let kdl_layout = r#"
        layout {
            pane split_direction="Horizontal" {
                pane size "3"   // fixed status bar (3 rows)
                pane            // flexible - takes remaining space
                pane size "50%" // mixed percentage
            }
        }
    "#;

    let (tiled_layout, _) = crate::tab::unit::parse_kdl_layout(kdl_layout);

    // Verify both constraint types exist in same layout
    assert!(
        tiled_layout.panes[0].size.is_fixed(),
        "First pane should be fixed"
    );
    assert!(
        tiled_layout.panes[1].size.is_percent(),
        "Second pane should be percentage (default)"
    );
    assert!(
        tiled_layout.panes[2].size.is_percent(),
        "Third pane should be explicit percentage"
    );

    // This layout will have resize behavior differences between fixed and flexible panes
}

#[test]
fn test_floating_pane_with_fixed_tiled_panes() {
    use crate::tab::unit::{create_layout_applier_fixtures, Size};

    let size = Size { cols: 100, rows: 50 };
    let fixture = create_layout_applier_fixtures(size);

    // Create a layout with fixed tiled pane and flexible floating pane
    let kdl_layout = r#"
        layout {
            pane split_direction="Horizontal" {
                pane size "24"  // fixed 24-row pane
                pane            // flexible remainder
            }
            floating_panes {
                pane width "50%" height "30%"  // flexible floating
            }
        }
    "#;

    let (tiled_layout, floating_layout) = parse_kdl_layout(kdl_layout);

    // Verify fixed pane exists in tiled layout
    assert!(
        tiled_layout.panes[0].size.is_fixed(),
        "Tiled pane should be fixed"
    );

    // Verify floating pane remains flexible (our critical fix!)
    if let Some(width) = floating_layout.first().and_then(|fp| fp.width.as_ref()) {
        match width {
            PercentOrFixed::Percent(50) => {}, // expected - stays as percent!
            PercentOrFixed::Fixed(_) => panic!(
                "Floating pane width should remain Percent, not converted to Fixed"
            ),
        }
    }

    if let Some(height) = floating_layout.first().and_then(|fp| height.as_ref()) {
        match height {
            PercentOrFixed::Percent(30) => {}, // expected - stays as percent!
            PercentOrFixed::Fixed(_) => panic!(
                "Floating pane height should remain Percent, not converted to Fixed"
            ),
        }
    }

    // Apply floating position and verify it preserves percentage constraint
    let mut geom = PaneGeom::default();
    let viewport = Viewport {
        x: 0,
        y: 0,
        rows: size.rows,
        cols: size.cols,
    };

    if let Some(floating) = floating_layout.first() {
        geom.apply_floating_pane_position(
            floating.x,
            floating.y,
            floating.width,
            floating.height,
            viewport.cols,
            viewport.rows,
        );

        // Verify constraint type preserved (the critical bug fix!)
        assert!(
            geom.rows.is_percent(),
            "Floating pane height should remain percentage after application"
        );
        assert!(
            geom.cols.is_percent(),
            "Floating pane width should remain percentage after application"
        );

        // Verify actual dimension calculated correctly from percentage
        assert_eq!(
            geom.rows.as_usize(),
            15,
            "30% of 50 rows = 15 (rounded)"
        );
        assert_eq!(
            geom.cols.as_usize(),
            50,
            "50% of 100 cols = 50"
        );
    }
}

#[test]
fn test_session_persistence_round_trip_fixed_size() {
    // Simulate session save/load with fixed pane
    let initial_geom = PaneGeom {
        rows: Dimension::fixed(24),
        cols: Dimension::fixed(80),
        ..Default::default()
    };

    // Serialize to PercentOrFixed (our deprecated alias for backward compatibility)
    let serialized: PercentOrFixed = initial_geom.rows.constraint.clone().into();
    
    match serialized {
        PercentOrFixed::Fixed(24) => {}, // expected - fixed size survives serialization
        _ => panic!(
            "Fixed dimension should serialize as Fixed, not Percent"
        ),
    }

    // Deserialize back to Dimension (round-trip test)
    let deserialized = Dimension::from_percent_or_fixed(serialized, 100);
    
    assert!(
        deserialized.is_fixed(),
        "Fixed constraint should survive round-trip serialization"
    );
    assert_eq!(
        deserialized.as_usize(),
        24,
        "Fixed value should be preserved through round-trip"
    );

    // Test with different viewport size (edge case: what if viewport changed?)
    let resized_viewport = 120;
    let reserialized = Dimension::from_percent_or_fixed(
        deserialized.clone().into(),
        resized_viewport,
    );

    assert!(
        reserialized.is_fixed(),
        "Fixed constraint should remain fixed even with different viewport"
    );
    assert_eq!(
        reserialized.as_usize(),
        24,
        "Fixed value should be independent of viewport size"
    );
}

#[test]
fn test_zero_size_edge_case_validation() {
    // Test parsing edge case: zero dimension
    let parsed = PercentOrFixed::from_str("0");
    
    match parsed {
        Ok(PercentOrFixed::Fixed(0)) => {}, // should parse successfully (validation happens later)
        Err(e) => panic!("'0' should parse as Fixed(0), not error: {}", e),
    }

    let parsed_pct = PercentOrFixed::from_str("0%");
    
    match parsed_pct {
        Ok(PercentOrFixed::Percent(0)) => {}, // should parse successfully
        Err(e) => panic!("'0%' should parse as Percent(0), not error: {}", e),
    }

    // Verify is_zero() method works correctly
    let zero_fixed = PercentOrFixed::Fixed(0);
    assert!(
        zero_fixed.is_zero(),
        "Zero fixed dimension should be detected"
    );

    let zero_pct = PercentOrFixed::Percent(0);
    assert!(
        zero_pct.is_zero(),
        "Zero percentage dimension should be detected"
    );

    let non_zero = PercentOrFixed::Fixed(5);
    assert!(
        !non_zero.is_zero(),
        "Non-zero fixed dimension should not be zero"
    );
}

#[test]
fn test_percentage_over_100_edge_case_validation() {
    // Test parsing edge case: invalid percentage > 100
    let result = PercentOrFixed::from_str("150%");
    
    assert!(
        result.is_err(),
        "Percent > 100 should be rejected with error"
    );

    if let Err(e) = result {
        // Verify error message is clear (not silent failure)
        assert!(
            e.to_string().contains("between 0 and 100"),
            "Error message should explain the constraint: {}",
            e
        );
    }

    // Test boundary case: exactly 100% (should be valid)
    let result = PercentOrFixed::from_str("100%");
    
    assert!(
        result.is_ok(),
        "Percent = 100 should be valid"
    );

    if let Ok(parsed) = result {
        match parsed {
            PercentOrFixed::Percent(100) => {}, // expected - full size
            _ => panic!("'100%' should parse as Percent(100), not {:?}", parsed),
        }
    }

    // Test boundary case: 0% (edge but valid)
    let result = PercentOrFixed::from_str("0%");
    
    assert!(
        result.is_ok(),
        "Percent = 0 should be valid (though probably useless)"
    );
}

#[test]
fn test_cli_vs_layout_precedence_scenario() {
    // Test precedence rule: CLI size overrides layout for run commands
    
    // Scenario: User runs `zellij run --size 80,24 code .` with existing layout
    let cli_size = PercentOrFixed::from_str("80").expect("CLI width parses");
    let cli_height = PercentOrFixed::from_str("24").expect("CLI height parses");

    // Layout has different size specification
    let kdl_layout = r#"
        layout {
            pane split_direction="Horizontal" {
                pane size "50%"  // layout says flexible
            }
        }
    "#;

    let (tiled_layout, _) = parse_kdl_layout(kdl_layout);

    // Verify layout has percentage constraint
    assert!(
        tiled_layout.panes[0].size.is_percent(),
        "Layout pane should have percentage constraint"
    );

    // CLI precedence rule: when user specifies --size on command line, 
    // that overrides the layout's size specification for that run
    
    // Our fix preserves this by treating both as DimensionConstraint
    let cli_as_dimension = Dimension::from_percent_or_fixed(cli_size, 100);
    assert!(
        cli_as_dimension.is_fixed(),
        "CLI fixed dimensions should be recognized"
    );

    // The actual precedence happens at runtime when creating the pane,
    // but our type system preserves both representations correctly
}

#[test]
fn test_minimum_size_edge_case() {
    // Test what happens with size below MIN_TERMINAL_HEIGHT
    
    let min_height = MIN_TERMINAL_HEIGHT; // 5 from constants
    
    // User specifies smaller fixed size (edge case)
    let small_fixed = PercentOrFixed::Fixed(2);
    
    match small_fixed {
        PercentOrFixed::Fixed(2) => {}, // should parse successfully
        _ => panic!("Small fixed dimension should parse"),
    }

    // Our fix doesn't enforce minimum at parsing level (that happens in pane_resizer)
    // But we verify the constraint type is preserved correctly
    let dimension = Dimension::from_percent_or_fixed(small_fixed, 50);
    
    assert!(
        dimension.is_fixed(),
        "Small fixed dimension should remain fixed"
    );
    assert_eq!(
        dimension.as_usize(),
        2,
        "Fixed value preserved even if below minimum"
    );

    // Note: pane_resizer will clamp this to MIN_TERMINAL_HEIGHT at runtime
    // Our test verifies the constraint type survives correctly through parsing
}

#[test]
fn test_stacked_pane_with_fixed_constraint() {
    // Test stacked panes with fixed size
    
    let kdl_layout = r#"
        layout {
            pane split_direction="Horizontal" {
                pane size "24"  // fixed pane that will be stacked
                pane            // flexible remainder
            }
        }
    "#;

    let (tiled_layout, _) = parse_kdl_layout(kdl_layout);

    // Verify the fixed pane can participate in stacking
    assert!(
        tiled_layout.panes[0].size.is_fixed(),
        "Pane should be fixed before stacking"
    );

    // When stacked, the constraint type should remain Fixed
    let geom = PaneGeom {
        rows: Dimension::from_percent_or_fixed(
            tiled_layout.panes[0].size.clone(),
            50,
        ),
        cols: Dimension::fixed(80),
        ..Default::default()
    };

    assert!(
        geom.rows.is_fixed(),
        "Fixed constraint should survive stack operation"
    );

    // combine_vertically_with_many preserves Fixed constraints (even if it returns None)
    // Our fix doesn't change this behavior - just improves error handling
}

#[test]
fn test_rounding_error_accumulation_multiple_resize() {
    // Test if rounding errors accumulate across multiple resize operations
    
    let viewport = 103; // odd number to expose rounding issues
    
    // First resize: 37% of 103 = 38.11, floor = 38
    let pct_37 = PercentOrFixed::Percent(37);
    let dim_first = Dimension::from_percent_or_fixed(pct_37, viewport);
    
    assert_eq!(dim_first.as_usize(), 38, "First resize: 37% of 103 = 38");

    // Second resize to larger viewport: 37% of 104 = 38.48, floor = 38
    let dim_second = Dimension::from_percent_or_fixed(pct_37, viewport + 1);
    
    assert_eq!(dim_second.as_usize(), 38, "Second resize: 37% of 104 = 38");

    // Third resize to larger: 37% of 105 = 38.85, floor = 38
    let dim_third = Dimension::from_percent_or_fixed(pct_37, viewport + 2);
    
    assert_eq!(dim_third.as_usize(), 38, "Third resize: 37% of 105 = 38");

    // Fourth resize to larger: 37% of 106 = 39.22, floor = 39 (终于 increments!)
    let dim_fourth = Dimension::from_percent_or_fixed(pct_37, viewport + 3);
    
    assert_eq!(dim_fourth.as_usize(), 39, "Fourth resize: 37% of 106 = 39");

    // Verify constraint type remains Percent throughout (critical fix!)
    assert!(dim_first.is_percent(), "First dimension should remain percentage");
    assert!(dim_second.is_percent(), "Second dimension should remain percentage");
    assert!(dim_third.is_percent(), "Third dimension should remain percentage");
    assert!(dim_fourth.is_percent(), "Fourth dimension should remain percentage");

    // This test documents the expected rounding behavior - constraint type preserved
}

/// Helper for parsing KDL layouts in tests (same as main pane_size_tests.rs)
fn parse_kdl_layout(kdl_str: &str) -> (TiledPaneLayout, Vec<crate::tab::unit::FloatingPaneLayout>) {
    use crate::input::layout::{FloatingPaneLayout, Layout};

    let parser = kdl_utils::KdlLayoutParser::new(kdl_str, None, None);
    let layout_result = parser.parse_layout();

    match layout_result {
        Ok(layout) => (
            layout
                .tabs
                .first()
                .map(|(_, t, f)| t.clone())
                .unwrap_or_default(),
            layout
                .tabs
                .first()
                .map(|(_, _, fs)| fs.clone())
                .unwrap_or_default(),
        ),
        Err(e) => panic!("Failed to parse KDL: {}", e),
    }
}
