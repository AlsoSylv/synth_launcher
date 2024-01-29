pub use async_channel::TryRecvError;
use std::pin::Pin;

enum InternalMessage<M, R> {
    Message(M),
    Callback(Pin<Box<dyn std::future::Future<Output = ()> + Send>>),
    CallbackWithResponse(Pin<Box<dyn std::future::Future<Output = R> + Send>>),
    Future(Pin<Box<dyn std::future::Future<Output = R> + Send>>),
}

pub struct Runtime<M, R, S>
where
    S: Send + 'static,
    R: Send + 'static,
    M: Send + 'static,
{
    _rt: tokio::runtime::Runtime,
    tx: async_channel::Sender<InternalMessage<M, R>>,
    rx: async_channel::Receiver<R>,
    state: &'static S,
}

impl<M, R, S> Runtime<M, R, S>
where
    S: Send + Sync + 'static,
    R: Send + 'static,
    M: Send + 'static,
{
    /// Creates a new Runtime for egui, allowing you to define how you react to events
    /// in the form of returning a struct, which will then get sent back to your egui thread
    pub fn new<'a, F, T>(
        thread_count: usize,
        state: &'static S,
        ctx: eframe::egui::Context,
        event_loop: F,
        rt: tokio::runtime::Runtime,
    ) -> Runtime<M, R, S>
    where
        F: Fn(M, &'a S) -> T + Clone + Send + Sync + 'static,
        T: std::future::Future<Output = R> + Send + 'a,
    {
        let (tx, rx_thread) = async_channel::unbounded();
        let (tx_thread, rx) = async_channel::unbounded();

        for _ in 0..thread_count {
            let (tx, rx) = (tx_thread.clone(), rx_thread.clone());
            let event_loop = event_loop.clone();
            let ctx = ctx.clone();

            rt.spawn(async move {
                let ctx = ctx;
                loop {
                    if let Ok(i_message) = rx.recv().await {
                        match i_message {
                            InternalMessage::Message(message) => {
                                tx.send(event_loop(message, state).await).await.unwrap();
                            }
                            InternalMessage::Callback(mut fut) => {
                                let mut poll = futures::poll!(&mut fut);
                                while poll.is_pending() {
                                    poll = futures::poll!(&mut fut);
                                }
                            }
                            InternalMessage::CallbackWithResponse(fut) => {
                                tx.send(fut.await).await.unwrap();
                            }
                            InternalMessage::Future(future) => {
                                tx.send(future.await).await.unwrap();
                            }
                        }

                        ctx.request_repaint();
                    }
                }
            });
        }

        Runtime {
            _rt: rt,
            tx,
            rx,
            state,
        }
    }

    pub fn send_with_message(&self, msg: M) {
        self.tx
            .send_blocking(InternalMessage::Message(msg))
            .expect("There should be no way to close the channel on the other end here")
    }

    pub fn callback<F, Fut>(&self, callback: F)
    where
        F: Fn(&S) -> Fut,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        self.tx
            .send_blocking(InternalMessage::Callback(Box::pin(callback(self.state))))
            .expect("There should be no way to close the channel on the other end here")
    }

    pub fn callback_response<'a, F, Fut>(&self, callback: F)
    where
        F: Fn(&'a S) -> Fut,
        Fut: std::future::Future<Output = R> + Send + 'static,
    {
        self.tx
            .send_blocking(InternalMessage::CallbackWithResponse(Box::pin(callback(
                self.state,
            ))))
            .expect("There should be no way to close the channel on the other end here")
    }

    pub fn future<Fut>(&self, future: Fut)
    where
        Fut: std::future::Future<Output = R> + Send + 'static,
    {
        self.tx
            .send_blocking(InternalMessage::Future(Box::pin(future)))
            .expect("There should be no way to close the channel on the other end here");
    }

    pub fn try_recv(&self) -> Result<R, TryRecvError> {
        self.rx.try_recv()
    }
}

#[cfg(test)]
mod test {
    use futures::poll;
    use std::pin::pin;
    use std::task::Poll;
    use std::time::Duration;

    #[test]
    fn test() {
        let runtime = tokio::runtime::Runtime::new().unwrap();

        let future = runtime.spawn(async {
            let mut future = Box::pin(async { 10 });

            let mut poll = poll!(&mut future);

            while poll.is_pending() {
                poll = poll!(&mut future);
            }

            match poll {
                Poll::Ready(t) => t,
                Poll::Pending => unreachable!(),
            }
        });

        println!("{:?}", runtime.block_on(future));
    }

    #[test]
    fn cancel_test() {
        let (abort_handle, abort_registration) = futures::future::AbortHandle::new_pair();

        let rt = tokio::runtime::Runtime::new().unwrap();

        let cancel_future = futures::future::Abortable::new(
            async {
                for x in 0..10 {
                    println!("{x}");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            },
            abort_registration,
        );

        rt.spawn(async move {
            let mut cancel_future = pin!(cancel_future);

            for _ in 0..10 {
                if cancel_future.is_aborted() {
                    break;
                }
                _ = poll!(&mut cancel_future);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });

        std::thread::sleep(Duration::from_secs(2));

        abort_handle.abort();
    }
}
