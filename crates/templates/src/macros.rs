// Copyright 2024 New Vector Ltd.
// Copyright 2021-2024 The Matrix.org Foundation C.I.C.
//
// SPDX-License-Identifier: AGPL-3.0-only
// Please see LICENSE in the repository root for full details.

/// Count the number of tokens. Used to have a fixed-sized array for the
/// templates list.
macro_rules! count {
    () => (0_usize);
    ( $x:tt $($xs:tt)* ) => (1_usize + count!($($xs)*));
}

/// Macro that helps generating helper function that renders a specific template
/// with a strongly-typed context. It also register the template in a static
/// array to help detecting missing templates at startup time.
///
/// The syntax looks almost like a function to confuse syntax highlighter as
/// little as possible.
#[macro_export]
macro_rules! register_templates {
    {
        $(
            extra = { $( $extra_template:expr ),* $(,)? };
        )?

        $(
            // Match any attribute on the function, such as #[doc], #[allow(dead_code)], etc.
            $( #[ $attr:meta ] )*
            // The function name
            pub fn $name:ident
                // Optional list of generics. Taken from
                // https://newbedev.com/rust-macro-accepting-type-with-generic-parameters
                $(< $( $lt:tt $( : $clt:tt $(+ $dlt:tt )* )? ),+ >)?
                // Type of context taken by the template
                ( $param:ty )
            {
                // The name of the template file
                $template:expr
            }
        )*
    } => {
        /// List of registered templates
        static TEMPLATES: [&'static str; count!( $( $template )* )] = [ $( $template, )* ];

        impl Templates {
            $(
                $(#[$attr])?
                ///
                /// # Errors
                ///
                /// Returns an error if the template fails to render.
                pub fn $name
                    $(< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)?
                    (&self, context: &$param)
                -> Result<String, TemplateError> {
                    let ctx = ::minijinja::value::Value::from_serialize(context);

                    let env = self.environment.load();
                    let tmpl = env.get_template($template)
                        .map_err(|source| TemplateError::Missing { template: $template, source })?;
                    tmpl.render(ctx)
                        .map_err(|source| TemplateError::Render { template: $template, source })
                }
            )*
        }

        /// Helps rendering each template with sample data
        pub mod check {
            use super::*;

            $(
                #[doc = concat!("Render the `", $template, "` template with sample contexts")]
                ///
                /// # Errors
                ///
                /// Returns an error if the template fails to render with any of the sample.
                pub(crate) fn $name
                    $(< $( $lt $( : $clt $(+ $dlt )* + TemplateContext )? ),+ >)?
                    (templates: &Templates, now: chrono::DateTime<chrono::Utc>, rng: &mut impl rand::Rng)
                -> anyhow::Result<()> {
                    let locales = templates.translator().available_locales();
                    let samples: Vec< $param > = TemplateContext::sample(now, rng, &locales);

                    let name = $template;
                    for sample in samples {
                        let context = serde_json::to_value(&sample)?;
                        ::tracing::info!(name, %context, "Rendering template");
                        templates. $name (&sample)
                            .with_context(|| format!("Failed to render template {:?} with context {}", name, context))?;
                    }

                    Ok(())
                }
            )*
        }
    };
}
