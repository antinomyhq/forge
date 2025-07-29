use std::marker::PhantomData;

pub trait SemiGroup {
    fn combine(self, other: Self) -> Self;
}

pub trait Identity {
    fn identity() -> Self;
}

pub trait Monoid: SemiGroup + Identity {}

impl<T> Monoid for T where T: SemiGroup + Identity {}

pub trait Program {
    type State;
    type Action;
    type Success: Monoid;
    type Error;
    fn update(
        &self,
        action: &Self::Action,
        state: &mut Self::State,
    ) -> std::result::Result<Self::Success, Self::Error>;
}

pub trait ProgramExt: Program {
    fn combine<B>(
        self,
        other: B,
    ) -> impl Program<
        Action = Self::Action,
        State = Self::State,
        Success = Self::Success,
        Error = Self::Error,
    >
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
    fn combine<B>(
        self,
        other: B,
    ) -> impl Program<Action = A::Action, State = A::State, Success = A::Success, Error = A::Error>
    where
        B: Program<Action = A::Action, State = A::State, Success = A::Success, Error = A::Error>,
    {
        (self, other)
    }
}

impl<State, Action, Success: Monoid, Error> Program
    for PhantomData<(State, Action, Success, Error)>
{
    type State = State;
    type Action = Action;
    type Success = Success;
    type Error = Error;

    fn update(
        &self,
        _: &Self::Action,
        _: &mut Self::State,
    ) -> std::result::Result<Self::Success, Self::Error> {
        Ok(Success::identity())
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
        &self,
        action: &Self::Action,
        state: &mut Self::State,
    ) -> std::result::Result<Self::Success, Self::Error> {
        let output0 = self.0.update(action, state)?;
        let output1 = self.1.update(action, state)?;
        Ok(output0.combine(output1))
    }
}
