pub type RepeatCount = usize;

#[derive(Debug, Clone, Copy)]
pub enum Movement {
    BackwardChar(RepeatCount),
    BackwardWord(RepeatCount),
    ForwardChar(RepeatCount),
    ForwardWord(RepeatCount),
    StartOfLine,
    EndOfLine,
    None,
}

#[derive(Debug, Clone)]
pub enum Action {
    AcceptLine,
    Cancel,
    EndOfFile,
    InsertChar(RepeatCount, char),
    InsertText(RepeatCount, String),
    Repaint,
    Move(Movement),
    Kill(Movement),
    KillAndMove(Movement, Movement),
    HistoryPrevious,
    HistoryNext,
    Complete,
    NoAction,
    HistoryIncSearchBackwards,
    HistoryIncSearchForwards,
}
