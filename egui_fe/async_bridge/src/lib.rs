use std::pin::Pin;

enum InternalMessage<M, R> {
    Message(M),
    Callback(Pin<Box<dyn std::future::Future<Output = ()> + Send>>),
    CallbackWithResponse(Pin<Box<dyn std::future::Future<Output = R> + Send>>),
}

pub struct Runtime<M, R, S>
where
    S: Clone + Send + 'static,
    R: Send + 'static,
    M: Send + 'static,
{
    _rt: tokio::runtime::Runtime,
    tx: async_channel::Sender<InternalMessage<M, R>>,
    rx: async_channel::Receiver<R>,
    state: S,
}

impl<M, R, S> Runtime<M, R, S>
where
    S: Clone + Send + Sync + 'static,
    R: Send + 'static,
    M: Send + 'static,
{
    /// Creates a new Runtime for egui, allowiing you to define how you react to events
    /// in the form of returning a struct, which will then get sent back to your egui thread
    pub fn new<F, T>(
        thread_count: usize,
        state: S,
        ctx: eframe::egui::Context,
        event_loop: F,
        rt: tokio::runtime::Runtime,
    ) -> Runtime<M, R, S>
    where
        F: Fn(M, &S) -> T + Clone + Send + Sync + 'static,
        T: std::future::Future<Output = R> + Send + 'static,
    {
        let (tx, rx_thread) = async_channel::unbounded();
        let (tx_thread, rx) = async_channel::unbounded();

        for _ in 0..thread_count {
            let (tx, rx) = (tx_thread.clone(), rx_thread.clone());
            let event_loop = event_loop.clone();
            let state = state.clone();
            let ctx = ctx.clone();

            rt.spawn(async move {
                loop {
                    if let Ok(i_message) = rx.recv().await {
                        match i_message {
                            InternalMessage::Message(message) => {
                                tx.send(event_loop(message, &state).await).await.unwrap();
                                ctx.request_repaint();
                            }
                            InternalMessage::Callback(fut) => {
                                fut.await;
                            }
                            InternalMessage::CallbackWithResponse(fut) => {
                                tx.send(fut.await).await.unwrap();
                                ctx.request_repaint();
                            }
                        }
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

    pub fn send_with_callback<F, Fut>(&self, callback: F)
    where
        F: Fn(&S) -> Fut,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        self.tx
            .send_blocking(InternalMessage::Callback(Box::pin(callback(
                &self.state,
            ))))
            .expect("There should be no way to close the channel on the other end here")
    }

    pub fn send_with_callback_with_response<F, Fut>(&self, callback: F)
    where
        F: Fn(&S) -> Fut,
        Fut: std::future::Future<Output = R> + Send + 'static,
    {
        self.tx
            .send_blocking(InternalMessage::CallbackWithResponse(Box::pin(
                callback(&self.state),
            )))
            .expect("There should be no way to close the channel on the other end here")
    }

    pub fn try_recv(&self) -> Result<R, async_channel::TryRecvError> {
        self.rx.try_recv()
    }
}
