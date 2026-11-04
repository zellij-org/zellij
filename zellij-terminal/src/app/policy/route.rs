use zellij_utils::data::PaneId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachedWindow {
    pub key: u64,
    pub session: Option<String>,
    pub focused_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Raise {
        window: u64,
        pane: Option<PaneId>,
    },
    Open {
        session: String,
        pane: Option<PaneId>,
    },
}

pub fn route(session: &str, pane: Option<PaneId>, windows: &[AttachedWindow]) -> Route {
    windows
        .iter()
        .filter(|window| window.session.as_deref() == Some(session))
        .max_by_key(|window| (window.focused_at, window.key))
        .map(|window| Route::Raise {
            window: window.key,
            pane,
        })
        .unwrap_or_else(|| Route::Open {
            session: session.to_owned(),
            pane,
        })
}

pub fn pane_focus(pane: Option<PaneId>) -> Option<(u32, bool)> {
    pane.map(|pane| match pane {
        PaneId::Terminal(id) => (id, false),
        PaneId::Plugin(id) => (id, true),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(key: u64, session: &str, focused_at: u64) -> AttachedWindow {
        AttachedWindow {
            key,
            session: Some(session.to_owned()),
            focused_at,
        }
    }

    #[test]
    fn a_window_already_on_the_session_is_brought_forward_with_the_pane() {
        let windows = vec![window(1, "api", 5), window(2, "docs", 9)];
        assert_eq!(
            route("api", Some(PaneId::Terminal(3)), &windows),
            Route::Raise {
                window: 1,
                pane: Some(PaneId::Terminal(3))
            }
        );
    }

    #[test]
    fn the_most_recently_focused_of_several_windows_on_the_session_wins() {
        let windows = vec![
            window(1, "api", 5),
            window(2, "api", 9),
            window(3, "api", 2),
        ];
        assert_eq!(
            route("api", None, &windows),
            Route::Raise {
                window: 2,
                pane: None
            }
        );
    }

    #[test]
    fn with_no_window_on_the_session_a_new_window_opens_and_no_other_window_switches() {
        let windows = vec![
            window(1, "docs", 5),
            AttachedWindow {
                key: 2,
                session: None,
                focused_at: 9,
            },
        ];
        assert_eq!(
            route("api", Some(PaneId::Plugin(4)), &windows),
            Route::Open {
                session: "api".to_owned(),
                pane: Some(PaneId::Plugin(4))
            }
        );
        assert_eq!(
            route("api", None, &[]),
            Route::Open {
                session: "api".to_owned(),
                pane: None
            }
        );
    }

    #[test]
    fn a_pane_becomes_the_focus_a_new_window_attaches_with() {
        assert_eq!(pane_focus(Some(PaneId::Terminal(3))), Some((3, false)));
        assert_eq!(pane_focus(Some(PaneId::Plugin(2))), Some((2, true)));
        assert_eq!(pane_focus(None), None);
    }
}
