# Dot-prefixed paths, parent closes and top-level parameters.

top_level_parameter = 42

[./root]
  type = ParentBlock
  value = 1

  [./child]
    type = ChildBlock
    nested = 'yes'
  [../]
[../]
