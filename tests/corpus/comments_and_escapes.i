# Comments, escaped strings, unicode and inline annotations.

[GlobalParams]
  # A comment on its own line.
  displacements = 'disp_x disp_y' # inline comment
[]

[Materials]
  [./elasticity]
    type = ComputeLinearElasticity
    # Escaped quotes and backslashes.
    description = 'it\'s a "quoted" value with a backslash: \\'
    double_quoted = "a tab:\t and a newline:\n"
    unicode = 'résumé ✓'
    # A comment between parameters.
    youngs_modulus = 2.0e11
    poisson_ratio = 0.3
  [../]
[]

[Outputs]
  exodus = true # write an Exodus file
  [./console]
    type = Console
    execute_on = 'initial timestep_end'
  [../]
[]
