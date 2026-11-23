# Kessel

Kessel mines the SWTOR client: it reads what the game ships and turns it into knowledge that huttspawn and warden can use.

Named after the spice mines of Kessel.

## Status

The repository is restarting. The extractor that produced `spice.sqlite` through 2026 is kept whole on the `oracle` branch; this tree starts empty and grows as the new design asks for each piece.

The new design has three stores, in refining order:

- **spice:** what each artifact in the client says, decoded, with its bytes kept where a patch would otherwise lose them.
- **ryll:** the whole game, normalized into nodes and edges of knowledge across builds, with outside sources such as patch notes and guides riding along.
- **glitterstim:** the here and now, denormalized for what huttspawn and warden publish and build.
