pub use api::{
    admin_set_read_only, admin_withdraw, delete_file, get_chunk, get_status, put_chunk, stat,
};
use candid::Principal;
use ic_cdk::export_candid;
use ic_cdk_macros::{init, post_upgrade};
pub use ic_papi_api::PaymentType;
use shared::{
    types::{DownloadToken, FileId, UploadToken},
    CanisterStatus,
};

use crate::{
    config::Args,
    memory::{mutate_config, set_config},
    results::{
        AdminSetReadOnlyResult, AdminWithdrawResult, DeleteFileResult, GetChunkResult,
        PutChunkResult,
    },
};

export_candid!();
