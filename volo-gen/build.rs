fn main() {
    volo_build::ConfigBuilder::default().plugin(STBasePlugin::default()).write().unwrap();
}

use faststr::FastStr;
use itertools::Itertools;
use pilota_build::{
    db::RirDatabase,
    rir::{self, Field, Method, MethodSource, Path},
    ty::{
        Ty,
        TyKind::{self},
    },
    Context, DefId, IdentName, Symbol,
};
use std::{collections::HashMap, sync::Arc};
use tracing;
use volo_build::Plugin;

#[derive(Default)]
pub struct STBasePlugin {
    cached_items: HashMap<DefId, bool>,
}

impl Clone for STBasePlugin {
    fn clone(&self) -> Self {
        Self {
            cached_items: HashMap::new(),
        }
    }
}

impl STBasePlugin {
    fn method_ty_path(
        &self,
        cx: &Context,
        cur_def_id: DefId,
        service_name: Symbol,
        method: &Method,
        suffix: &str,
    ) -> FastStr {
        match method.source {
            MethodSource::Extend(def_id) => {
                let item = cx.expect_item(def_id);
                let target_service = match &*item {
                    rir::Item::Service(s) => s,
                    _ => panic!("expected service"),
                };
                let ident = format!(
                    "{}{}{}",
                    target_service.name,
                    cx.rust_name(method.def_id).0.upper_camel_ident(),
                    suffix,
                );
                let path = cx.related_item_path(cur_def_id, def_id);
                let mut path = path.split("::").collect_vec();

                path.pop();
                path.push(&*ident);
                let path = path.join("::");
                path.into()
            }
            rir::MethodSource::Own => {
                let ident = format!(
                    "{}{}{}",
                    service_name,
                    cx.rust_name(method.def_id).0.upper_camel_ident(),
                    suffix
                );
                ident.into()
            }
        }
    }

    fn method_result_path(
        &self,
        cx: &Context,
        cur_def_id: DefId,
        service_name: Symbol,
        method: &Method,
    ) -> FastStr {
        self.method_ty_path(cx, cur_def_id, service_name, method, "Result")
    }

    fn item_base_field(&self, cx: &volo_build::Context, def_id: DefId) -> Option<Arc<Field>> {
        let item = cx.item(def_id)?;

        let s = if let pilota_build::rir::Item::Message(s) = &*item {
            Some(s)
        } else {
            None
        }?;

        let field = s.fields.iter().find(|f| f.id == 255)?;

        let base_resp_ty = match &field.ty.kind {
            pilota_build::ty::TyKind::Path(Path { did, .. }) => Some(cx.item(*did)?),
            _ => None,
        }?;

        let base_resp_ty = match &*base_resp_ty {
            pilota_build::rir::Item::Message(m) => Some(m),
            _ => None,
        }?;

        let mut message_field = None;
        let mut status_code_field = None;
        let mut extra_field = None;

        if base_resp_ty.fields.len() != 3 {
            return None;
        }

        for field in &base_resp_ty.fields {
            match field.id {
                1 => message_field = Some(field),
                2 => status_code_field = Some(field),
                3 => extra_field = Some(field),
                _ => {}
            }
        }

        let is_string = |ty: &Ty| -> bool { matches!(ty.kind, TyKind::String | TyKind::FastStr) };

        message_field.filter(|f| is_string(&f.ty))?;
        status_code_field.filter(|f| f.ty.kind == TyKind::I32)?;
        extra_field.filter(|f| {
            if let TyKind::Map(k, v) = &f.ty.kind {
                is_string(k) && is_string(v)
            } else {
                false
            }
        })?;

        Some(field.clone())
    }
}

impl Plugin for STBasePlugin {
    fn on_item(
        &mut self,
        cx: &volo_build::Context,
        def_id: volo_build::DefId,
        item: std::sync::Arc<volo_build::rir::Item>,
    ) {
        if !matches!(cx.source_type, pilota_build::SourceType::Thrift) {
            return;
        }
        match &*item {
            volo_build::rir::Item::Service(s) => {
                let methods = cx.service_methods(def_id);
                let variant_bases = methods.iter().map(|m| {
                    let resp_ty = &m.ret;
                    match resp_ty.kind {
                        pilota_build::ty::TyKind::Path(Path { did, .. }) => {
                            let r = if let Some(r) = self.cached_items.get(&did) {
                                *r
                            } else {
                                let r = if let Some(field) = self.item_base_field(cx, did) {
                                    let field_name = cx.rust_name(field.did);
                                    let name = cx.rust_name(did);
                                    let base = if field.is_optional() {
                                        format! {
                                            r#"
                                            self.{field_name}.as_ref().map(|base_resp| {{
                                                BaseResp {{
                                                    status_message: base_resp.status_message.clone(),
                                                    status_code: base_resp.status_code,
                                                    extra: base_resp.extra.clone(),
                                                }}
                                            }})
                                            "#
                                        }
                                    } else {
                                        format! {
                                            r#"
                                            Some(BaseResp {{
                                                status_message: self.{field_name}.status_message.clone(),
                                                status_code: self.{field_name}.status_code,
                                                extra: self.{field_name}.extra.clone(),
                                            }})
                                            "#
                                        }
                                    };

                                    cx.with_adjust_mut(did, |adj| {
                                        tracing::debug!("impl SetBaseResp for {:?}", name);
                                        adj.add_nested_item(format! {
                                            r#"
                                            impl {name} {{
                                                pub fn get_base_resp(&self) -> Option<BaseResp> {{
                                                    {base}
                                                }}
                                            }}
                                            "#
                                        }.into())
                                    });

                                    true
                                } else {
                                    false
                                };

                                self.cached_items.insert(did, r);
                                r
                            };

                            if r {
                                "value.get_base_resp()"
                            } else {
                                "None"
                            }
                        }
                        _ => "None",
                    }
                }).collect::<Vec<_>>();

                let res_name = format!("{}Response", cx.rust_name(def_id));

                let variant_names = methods
                    .iter()
                    .map(|m| cx.rust_name(m.def_id).0.upper_camel_ident())
                    .collect::<Vec<_>>();

                let service_name = cx.rust_name(def_id);

                let result_names = methods
                    .iter()
                    .map(|m| self.method_result_path(cx, def_id, service_name.clone(), m))
                    .collect::<Vec<_>>();

                cx.with_adjust_mut(def_id, |adj| {
                    ["Recv", "Send"]
                        .into_iter()
                        .for_each(|suffix| {
                            let res_name = format!("{res_name}{suffix}");
                            let variants = variant_names
                                .iter()
                                .zip_eq(result_names.iter())
                                .zip_eq(variant_bases.iter())
                                .map(|((variant_name, result_name), variant_base)| {
                                    let result_name = format!("{}{}", result_name, suffix);
                                    format!("Self::{variant_name}({result_name}::Ok(value)) => {variant_base},")
                                })
                                .join("");
                            // 1042 行
                            adj.add_nested_item(
                                format! {
                                    r#"
                                    impl {res_name} {{
                                        pub fn get_base_resp(&self) -> Option<BaseResp> {{
                                        match self {{
                                            {variants}
                                            _ => None,
                                        }}
                                    }}
                                    }}
                                "#
                                }
                                    .into(),
                            )
                        });
                });
            }
            _ => {}
        }
        volo_build::plugin::walk_item(self, cx, def_id, item)
    }
}

