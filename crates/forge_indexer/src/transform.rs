pub trait Transform {
    type In;
    type Out;
    fn transform(self, input: Self::In) -> impl Future<Output = anyhow::Result<Self::Out>>;
}

pub trait TransformOps: Sized {
    fn pipe<Other>(self, other: Other) -> Pipe<Self, Other> {
        Pipe(self, other)
    }

    fn map<F, TOut>(self, f: F) -> Map<Self, F>
    where
        F: Fn(Self::Out) -> TOut,
        Self: Transform,
    {
        Map { inner: self, func: f }
    }
}

impl<T: Transform> TransformOps for T {}

pub struct Pipe<T1, T2>(T1, T2);

impl<T1: Transform, T2: Transform> Transform for Pipe<T1, T2>
where
    T1::Out: Into<T2::In>,
{
    type In = T1::In;
    type Out = T2::Out;
    async fn transform(self, input: Self::In) -> anyhow::Result<Self::Out> {
        let intermediate_result = self.0.transform(input).await?;
        self.1.transform(intermediate_result.into()).await
    }
}

pub struct Map<T, F> {
    inner: T,
    func: F,
}

impl<T, F, TOut> Transform for Map<T, F>
where
    T: Transform,
    F: Fn(T::Out) -> TOut,
{
    type In = T::In;
    type Out = TOut;

    async fn transform(self, input: Self::In) -> anyhow::Result<Self::Out> {
        let result = self.inner.transform(input).await?;
        Ok((self.func)(result))
    }
}
