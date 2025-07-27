pub trait SemiGroup {
    fn combine(self, other: Self) -> Self;
}

pub trait Monoid: SemiGroup {
    fn identity() -> Self;
}

pub trait Program {
    type State;
    type Action;
    type Success: Monoid;
    type Error;
    fn update(
        self,
        action: &Self::Action,
        state: &mut Self::State,
    ) -> std::result::Result<Self::Success, Self::Error>;
}

trait ProgramExt: Program {
    fn combine<B: Program>(self, other: B) -> impl Program
    where
        B: Program<
                Action = Self::Action,
                State = Self::State,
                Success = Self::Success,
                Error = Self::Error,
            >;
}

impl<A: Program> ProgramExt for A
where
    A: Program,
{
    fn combine<B: Program>(self, other: B) -> impl Program
    where
        B: Program<Action = A::Action, State = A::State, Success = A::Success, Error = A::Error>,
    {
        (self, other)
    }
}

impl<A: Program, B> Program for (A, B)
where
    B: Program<Action = A::Action, State = A::State, Success = A::Success, Error = A::Error>,
{
    type State = A::State;
    type Action = A::Action;
    type Success = A::Success;
    type Error = A::Error;

    fn update(
        self,
        action: &Self::Action,
        state: &mut Self::State,
    ) -> std::result::Result<Self::Success, Self::Error> {
        let output0 = self.0.update(action, state)?;
        let output1 = self.1.update(action, state)?;
        Ok(output0.combine(output1))
    }
}
