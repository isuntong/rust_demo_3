#![feature(impl_trait_in_assoc_type)]

use lazy_static::lazy_static;
use std::net::SocketAddr;
use futures::Future;
use volo_thrift::{ApplicationError, ApplicationErrorKind};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt};

use rust_demo_3::{S};

#[volo::main]
async fn main() {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .init();
    let req = volo_gen::rust_demo_2::GetItemBackendRequest{ id: 0 };
    let resp = CLIENT.clone().get_item_backend(req).await;
    match resp {
        Ok(res) => {
            tracing::info!("get_item_backend resp: {:?}", res)
        }
        Err(e) => {
            tracing::info! ("get_item_backend error: {:?}", e)
        }
    }
}

lazy_static! {
	static ref CLIENT: volo_gen::rust_demo_2::ItemBackendServiceClient = {
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        volo_gen::rust_demo_2::ItemBackendServiceClientBuilder::new("rust_demo_2")
            .layer_outer_front(ErrorFrontLayer)
            .address(addr)
            .build()
    };
}

pub struct ErrorFrontLayer;

impl<S> volo::Layer<S> for ErrorFrontLayer {
    type Service = ErrorFrontService<S>;

    fn layer(self, inner: S) -> Self::Service {
        ErrorFrontService(inner)
    }
}

#[derive(Clone)]
pub struct ErrorFrontService<S>(S);

impl<Cx, Req, S> volo::Service<Cx, Req> for ErrorFrontService<S>
    where
        Req: std::fmt::Debug + Send + 'static, // 特征约束 https://course.rs/basic/trait/trait.html
        S: Send + Sync + 'static + volo::Service<Cx, Req, Error = volo_thrift::Error, Response = Option<volo_gen::rust_demo_2::ItemBackendServiceResponseRecv>>,
        S::Response: std::fmt::Debug,
        S::Error: std::fmt::Debug,
        Cx: Send + 'static + volo::context::Context,
{

    type Response = S::Response; // 关联类型
type Error = S::Error;
    type Future<'cx> = impl Future<Output = Result<Self::Response, Self::Error>> + Send + 'cx where S: 'cx;

    #[inline]
    fn call<'cx, 's>(&'s self, cx: &'cx mut Cx, req: Req) -> Self::Future<'cx>
        where
            's: 'cx,
    {
        async move {
            let now = std::time::Instant::now();
            tracing::info!("Rust Front Layer Request {:?}", &req);
            let resp = self.0.call(cx, req).await;
            tracing::info!("Rust Front Layer response {:?}", &resp);
            tracing::info!("Rust Front Layer took {}ms", now.elapsed().as_millis());

            // 处理返回的结果
            return match resp {
                Err(e) => {
                    Err(e)
                }
                Ok(r) => {
                    tracing::info!("Rust Front Layer Find Resp!");
                    let r_option:Option<volo_gen::rust_demo_2::ItemBackendServiceResponseRecv> = r;
                    // Resp不见了, 默认成功
                    if r_option.is_none() {
                        return Ok(r_option)
                    }
                    let r_real = r_option.clone().unwrap();
                    let base_resp_option = r_real.get_base_resp();
                    // BaseResp不见了, 默认成功
                    if base_resp_option.is_none() {
                        return Ok(r_option)
                    }
                    let base_resp_real = base_resp_option.unwrap();
                    let status_code_option = base_resp_real.status_code;
                    // StatusCode不见了, 默认成功
                    if status_code_option.is_none() {
                        return Ok(r_option)
                    }
                    let status_code_real = status_code_option.unwrap();
                    return match status_code_real {
                        12345678 => {
                            tracing::info!("Rust Front Layer Transform!");
                            Err(volo_thrift::Error::Application(volo_thrift::ApplicationError{kind: ApplicationErrorKind::UNKNOWN_METHOD, message: "Good Morning".into()}))
                        },
                        _ => {
                            Err(volo_thrift::Error::Application(volo_thrift::ApplicationError{kind: ApplicationErrorKind::UNKNOWN_METHOD, message: "Good Evening".into()}))
                        }
                    };
                }
            }
        }
    }
}