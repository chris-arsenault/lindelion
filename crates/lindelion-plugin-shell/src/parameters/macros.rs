/// Defines the public host parameter list and typed registry binding list for a plugin.
///
/// Plugin-specific patch paths, apply policy, runtime target, smoothing metadata, formatter,
/// and editor metadata stay in the caller. The macro only owns the shared registry shape.
#[macro_export]
macro_rules! define_parameter_bindings {
    (
        binding: $binding:ty;
        parameters: $parameters_vis:vis const $parameters_name:ident;
        bindings: $bindings_vis:vis const $bindings_name:ident;
        defaults {
            runtime: $default_runtime:expr,
            smoothing: $default_smoothing:expr $(,)?
        }

        $(
            $info:expr => {
                path: $path:expr,
                apply: $apply:expr,
                $(runtime: $runtime:expr,)?
                $(smoothing: $smoothing:expr,)?
                format: $format:expr,
                editor: $editor:expr $(,)?
            }
        ),+ $(,)?
    ) => {
        $parameters_vis const $parameters_name: &[$crate::parameters::ParameterInfo] = &[
            $($info),+
        ];

        $bindings_vis const $bindings_name: &[$binding] = &[
            $(<$binding>::new(
                $info,
                $path,
                $apply,
                $crate::__define_parameter_bindings_runtime!($default_runtime $(, $runtime)?),
                $crate::__define_parameter_bindings_smoothing!($default_smoothing $(, $smoothing)?),
                $format,
                $editor,
            )),+
        ];
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __define_parameter_bindings_runtime {
    ($default:expr) => {
        $default
    };
    ($default:expr, $runtime:expr) => {
        $runtime
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __define_parameter_bindings_smoothing {
    ($default:expr) => {
        $default
    };
    ($default:expr, $smoothing:expr) => {
        Some($smoothing)
    };
}

/// Defines the plain-value/index/label mapping for a stepped parameter enum.
#[macro_export]
macro_rules! define_parameter_codec {
    (
        impl $trait:ident for $type:ty {
            max: $max:expr;
            fallback: $fallback:path;
            $($index:literal => $variant:path, $label:expr;)+
        }
    ) => {
        impl $trait for $type {
            const MAX_INDEX: u32 = $max;
            const LABELS: &'static [&'static str] = &[$($label),+];

            fn from_index(index: u32) -> Self {
                match index {
                    $($index => $variant,)+
                    _ => $fallback,
                }
            }

            fn to_index(self) -> u32 {
                match self {
                    $($variant => $index,)+
                }
            }

            fn label(self) -> &'static str {
                match self {
                    $($variant => $label,)+
                }
            }
        }
    };
}
