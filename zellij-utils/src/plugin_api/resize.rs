pub use super::generated_api::api::resize::{
    MoveDirection as ProtobufMoveDirection, Resize as ProtobufResize, ResizeAction,
    ResizeDirection, ResizeDirection as ProtobufResizeDirection,
};
use crate::data::{Direction, Resize, ResizeStrategy};

use std::convert::TryFrom;

impl TryFrom<ProtobufResize> for Resize {
    type Error = &'static str;
    fn try_from(protobuf_resize: ProtobufResize) -> Result<Self, &'static str> {
        if protobuf_resize.direction.is_some() {
            return Err("Resize cannot have a direction");
        }
        match ResizeAction::from_i32(protobuf_resize.resize_action) {
            Some(ResizeAction::Increase) => Ok(Resize::Increase),
            Some(ResizeAction::Decrease) => Ok(Resize::Decrease),
            None => Err("No resize action for the given index"),
        }
    }
}

impl TryFrom<Resize> for ProtobufResize {
    type Error = &'static str;
    fn try_from(resize: Resize) -> Result<Self, &'static str> {
        Ok(ProtobufResize {
            resize_action: match resize {
                Resize::Increase => ResizeAction::Increase as i32,
                Resize::Decrease => ResizeAction::Decrease as i32,
            },
            direction: None,
        })
    }
}

impl TryFrom<ProtobufResize> for ResizeStrategy {
    type Error = &'static str;
    fn try_from(protobuf_resize: ProtobufResize) -> Result<Self, &'static str> {
        let direction = match protobuf_resize
            .direction
            .and_then(|r| ResizeDirection::from_i32(r))
        {
            Some(ResizeDirection::Left) => Some(Direction::Left),
            Some(ResizeDirection::Right) => Some(Direction::Right),
            Some(ResizeDirection::Up) => Some(Direction::Up),
            Some(ResizeDirection::Down) => Some(Direction::Down),
            None => None,
        };
        let resize = match ResizeAction::from_i32(protobuf_resize.resize_action) {
            Some(ResizeAction::Increase) => Resize::Increase,
            Some(ResizeAction::Decrease) => Resize::Decrease,
            None => return Err("No resize action for the given index"),
        };
        Ok(ResizeStrategy {
            direction,
            resize,
            invert_on_boundaries: false,
        })
    }
}

impl TryFrom<ResizeStrategy> for ProtobufResize {
    type Error = &'static str;
    fn try_from(resize_strategy: ResizeStrategy) -> Result<Self, &'static str> {
        Ok(ProtobufResize {
            resize_action: match resize_strategy.resize {
                Resize::Increase => ResizeAction::Increase as i32,
                Resize::Decrease => ResizeAction::Decrease as i32,
            },
            direction: match resize_strategy.direction {
                Some(Direction::Left) => Some(ResizeDirection::Left as i32),
                Some(Direction::Right) => Some(ResizeDirection::Right as i32),
                Some(Direction::Up) => Some(ResizeDirection::Up as i32),
                Some(Direction::Down) => Some(ResizeDirection::Down as i32),
                None => None,
            },
        })
    }
}

impl TryFrom<ProtobufMoveDirection> for Direction {
    type Error = &'static str;
    fn try_from(protobuf_move_direction: ProtobufMoveDirection) -> Result<Self, &'static str> {
        match ResizeDirection::from_i32(protobuf_move_direction.direction) {
            Some(ResizeDirection::Left) => Ok(Direction::Left),
            Some(ResizeDirection::Right) => Ok(Direction::Right),
            Some(ResizeDirection::Up) => Ok(Direction::Up),
            Some(ResizeDirection::Down) => Ok(Direction::Down),
            None => Err("No direction for the given index"),
        }
    }
}

impl TryFrom<Direction> for ProtobufMoveDirection {
    type Error = &'static str;
    fn try_from(direction: Direction) -> Result<Self, &'static str> {
        Ok(ProtobufMoveDirection {
            direction: match direction {
                Direction::Left => ResizeDirection::Left as i32,
                Direction::Right => ResizeDirection::Right as i32,
                Direction::Up => ResizeDirection::Up as i32,
                Direction::Down => ResizeDirection::Down as i32,
            },
        })
    }
}

impl TryFrom<ProtobufResizeDirection> for Direction {
    type Error = &'static str;
    fn try_from(protobuf_resize_direction: ProtobufResizeDirection) -> Result<Self, &'static str> {
        match protobuf_resize_direction {
            ProtobufResizeDirection::Left => Ok(Direction::Left),
            ProtobufResizeDirection::Right => Ok(Direction::Right),
            ProtobufResizeDirection::Up => Ok(Direction::Up),
            ProtobufResizeDirection::Down => Ok(Direction::Down),
        }
    }
}

impl TryFrom<Direction> for ProtobufResizeDirection {
    type Error = &'static str;
    fn try_from(direction: Direction) -> Result<Self, &'static str> {
        Ok(match direction {
            Direction::Left => ProtobufResizeDirection::Left,
            Direction::Right => ProtobufResizeDirection::Right,
            Direction::Up => ProtobufResizeDirection::Up,
            Direction::Down => ProtobufResizeDirection::Down,
        })
    }
}
