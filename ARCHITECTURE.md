# mFC Paper Integration (Doohan et al. 2026)

## Factorized Modules
- PolicyLibrary: compressed trajectory basis (NMF-style on behavior graphs)
- DistanceEvaluator: goal-tiled shortest-path scorer

## Theta Loop
for cycle in 1..8:
  futures = structured.sample()
  scores = distance.evaluate(futures)
  policy.update(scores)

## Ablation & Demo
See tests/maze_planning.py - full planning vs habit fallback.

Maps directly to paper Figs 4-7.