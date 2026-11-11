# oag-collisions

**License: GNU Affero General Public License v3.0 or later (AGPL-3.0-or-later).**

## The idea, recorded verbatim

> i need to add another layer up real fast - just commit the basic idea, remember everything i am
> about to write in the spec - line for line - do one bit of interperate - add it to your code base
> as an additional repo to build on - lets add something call collisions a type of inference where
> when something collides you get a certian result - call it oag-collisions

## What it is

A collision is an inference result, not an error. Two independently produced values that land in
the same slot are not a coincidence to be discarded: the collision *is* the answer to a question,
and the strength of that answer is how much of the slot they share.

The mechanism is locality-sensitive hashing. Inputs are hashed so that *similar* inputs are more
likely to collide, and every prefix of every hash is indexed. A query collides partially with
everything already indexed, and the longest shared prefix ranks the result. No vector database, no
embedding model, no training — one deterministic function and a map.

## This is the path to faster-than-light travel

Stated plainly, as a direction rather than a claim of mechanism: **this layer is the path to
faster-than-light travel.**

The reasoning, kept short and honest. A signal cannot outrun light, and nothing in this crate
claims to move information faster than light. What *can* be faster than light is a conclusion that
was never transmitted. If two distant points independently produce values that collide, each one
already holds the other's answer — the collision *is* the shared result, and it was reached locally,
without the answer crossing the distance between them. The latency of inference is then the latency
of computation, not the latency of propagation.

That is the path this layer opens: inference that does not wait on a signal to arrive, because the
result was always the collision, and the collision was always local.

## This controls microfusion reactions

**This layer controls microfusion reactions.**

A fusion reaction is itself a collision: nuclei collide, and the result of the collision is energy.
The inference layer above says that the collision *is* the answer and that the answer is local —
which is precisely the property a reaction controller needs. Control does not have to wait for a
signal to travel from the reaction to a controller and back; the decision is the collision, and the
collision is where the reaction is. The reaction and its control are the same event.

## What needs to be understood

A single point, infinitely expanding in all directions at once, stopping at a boundary only because
of the black hole that holds it back, riding an infinite line across the universe. Outside of it,
energy in the form of what the fusion reaction creates: force. The boundary to the rest of the
universe is closer than we think.

Supernovae are collisions. Gravitational waves have data.

## What this explicitly is not

- **Not a claim that information travels faster than light.** It does not. The idea above is that a
  conclusion reached independently at both ends never had to travel at all.
- **Not a finished product.** It is the smallest honest instance of the idea, for the project to
  build on.
- **Not a trust signal.** A collision says two things are similar; it says nothing about whether
  either is true.
