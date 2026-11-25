//! One capability that is a program's own contract AND `FormSource`: the bootstrap of a connection answers both on the one socket.
//!
//! Pattern: Decorator - the same shape as `Controlled` (`Control` next to the program's capability): the calls of `FormSource`
//! go to its capability, every other call goes, unchanged, to the program's own capability.

use std::rc::Rc;

use capnp::any_pointer;
use capnp::capability::{Client, DispatchCallResult, FromServer, Params, Promise, Results, Server};
use capnp::traits::HasTypeId;
use fs_interfaces::formsource_capnp::form_source;

struct WithForm {
    other: Client,
    form: Client,
}

#[derive(Clone)]
struct Dispatch(Rc<WithForm>);

impl FromServer<WithForm> for Client {
    type Dispatch = Dispatch;
    fn from_server(s: Rc<WithForm>) -> Dispatch {
        Dispatch(s)
    }
}

impl std::ops::Deref for Dispatch {
    type Target = WithForm;
    fn deref(&self) -> &WithForm {
        &self.0
    }
}

impl Server for Dispatch {
    fn dispatch_call(
        self,
        interface_id: u64,
        method_id: u16,
        params: Params<any_pointer::Owned>,
        mut results: Results<any_pointer::Owned>,
    ) -> DispatchCallResult {
        let to = if interface_id == <form_source::Client as HasTypeId>::TYPE_ID {
            &self.0.form
        } else {
            &self.0.other
        };
        let mut request =
            to.new_call::<any_pointer::Owned, any_pointer::Owned>(interface_id, method_id, None);
        DispatchCallResult::new(
            Promise::from_future(async move {
                request.get().set_as(params.get()?)?;
                let response = request.send().promise.await?;
                results.get().set_as(response.get()?)?;
                Ok(())
            }),
            false,
        )
    }

    fn as_ptr(&self) -> usize {
        Rc::as_ptr(&self.0) as usize
    }
}

/// One capability that answers `other` (the program's own contract) and `FormSource` (`form`, e.g. from
/// [`Guarded::connect`](crate::Guarded::connect)): `FormSource` calls go to `form`, every other call to `other`.
/// Everything runs in the `LocalSet` of the program (capnp-rpc).
#[must_use]
pub fn with_form_source(other: Client, form: form_source::Client) -> Client {
    capnp_rpc::new_client::<Client, _>(WithForm {
        other,
        form: form.client,
    })
}
