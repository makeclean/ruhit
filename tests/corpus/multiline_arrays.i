# Multiline bracket arrays and comma-separated values.

[Block]
  # A multiline array with comments between elements.
  values = [
    # first group
    1, 2, 3, # the first three
    4,
    5,
  ]

  # A nested array.
  matrix = [
    [1, 2],
    [3, 4],
    [5, 6],
  ]

  # Unbracketed comma-separated vectors.
  times = 0.0, 0.25, 0.5, 0.75, 1.0
  flags = true false true
  labels = one two three

  # Space-separated quoted vector, the traditional HIT spelling.
  boundary = 'left right top bottom'
[]
