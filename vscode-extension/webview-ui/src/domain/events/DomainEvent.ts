import { Schema as S } from "effect";

/// DomainEvent is the base class for all domain events
export abstract class DomainEvent extends S.Class<DomainEvent>("DomainEvent")({
  id: S.UUID,
  occurredAt: S.Date,
  aggregateId: S.String,
}) {}
