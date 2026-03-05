#[doc(hidden)]
#[macro_export]
macro_rules! vendored_termwiz_builder {
    (
        $( #[ $( $meta:tt )* ] )*
        $vis:vis struct $name:ident {
            $(
                $( #[doc=$doc:expr] )*
                $field:ident : $type:ty,
            )*
        }
    ) => {
        $( #[ $( $meta )* ] )*
        $vis struct $name {
            $(
                $( #[doc=$doc] )*
                $field : $type,
            )*
        }

        impl $name {
            $(
                pub fn $field(mut self, value: $type) -> Self {
                    self.$field = value;
                    self
                }
            )*
        }
    }
}
