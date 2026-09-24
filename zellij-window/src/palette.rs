use zellij_utils::ipc::{ClientToServerMsg, ColorRegister};

use crate::color::{self, Paints};

pub fn seed_messages(paints: &Paints) -> Vec<ClientToServerMsg> {
    vec![
        ClientToServerMsg::ForegroundColor {
            color: xparse_color(paints.foreground),
        },
        ClientToServerMsg::BackgroundColor {
            color: xparse_color(paints.background),
        },
        ClientToServerMsg::ColorRegisters {
            color_registers: color_registers(paints),
        },
    ]
}

fn color_registers(paints: &Paints) -> Vec<ColorRegister> {
    (0..=255u8)
        .map(|index| ColorRegister {
            index: index as usize,
            color: xparse_color(paints.indexed(index)),
        })
        .collect()
}

pub fn xparse_color([r, g, b]: color::Srgb) -> String {
    format!(
        "rgb:{:04x}/{:04x}/{:04x}",
        (r as u16) * 0x0101,
        (g as u16) * 0x0101,
        (b as u16) * 0x0101,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paints() -> Paints {
        Paints::default()
    }

    #[test]
    fn every_palette_index_is_seeded_exactly_once() {
        let registers = color_registers(&paints());
        assert_eq!(registers.len(), 256);
        for (expected_index, register) in registers.iter().enumerate() {
            assert_eq!(register.index, expected_index);
        }
    }

    #[test]
    fn registers_use_the_xparse_wire_shape() {
        for register in color_registers(&paints()) {
            assert!(register.color.starts_with("rgb:"));
            assert_eq!(register.color.len(), "rgb:0000/0000/0000".len());
        }
    }

    #[test]
    fn the_seed_carries_the_same_colors_the_renderer_resolves() {
        let registers = color_registers(&paints());
        assert_eq!(registers[1].color, xparse_color(color::ANSI_16[1]));
        assert_eq!(registers[208].color, xparse_color(color::indexed(208)));
    }

    #[test]
    fn a_configured_table_is_what_the_server_is_told_about() {
        let mut paints = paints();
        paints.ansi[1] = [1, 2, 3];
        paints.background = [4, 5, 6];
        let seeded = seed_messages(&paints);
        assert_eq!(
            seeded[1],
            ClientToServerMsg::BackgroundColor {
                color: xparse_color([4, 5, 6])
            }
        );
        assert_eq!(color_registers(&paints)[1].color, xparse_color([1, 2, 3]));
    }
}
