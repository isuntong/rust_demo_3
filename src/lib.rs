#![feature(impl_trait_in_assoc_type)]

pub struct S;

#[volo::async_trait]
impl volo_gen::rust_demo_3::ItemFrontService for S {
	async fn get_item_front(&self, _req: volo_gen::rust_demo_3::GetItemFrontRequest) -> ::core::result::Result<volo_gen::rust_demo_3::GetItemFrontResponse, ::volo_thrift::AnyhowError>{
		Ok(Default::default())
	}
}

