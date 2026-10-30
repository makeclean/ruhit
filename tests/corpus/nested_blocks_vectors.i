# Nested blocks, vectors, booleans and scientific notation.

[GlobalParams]
  displacements = 'disp_x disp_y'
  use_displaced_mesh = false
[]

[Mesh]
  type = FileMesh
  file = 'mesh.e'
  [./refinement]
    type = UniformRefinement
    levels = 2
    [./inner]
      enabled = true
    [../]
  [../]
[]

[Variables]
  [./disp_x]
    order = SECOND
    family = LAGRANGE
  [../]
  [./disp_y]
    order = SECOND
    family = LAGRANGE
  [../]
[]

[Materials]
  [./elasticity]
    type = ComputeLinearElasticity
    block = '1 2 3'
    youngs_modulus = 2.0e11
    poisson_ratio = 0.3
    plane_strain = false
    [./thermal]
      type = ComputeThermalExpansion
      thermal_expansion = 1.2E-5
      stress_free_temperature = 300.0
    [../]
  [../]
[]

[VectorPostprocessors]
  [./samples]
    type = PointValueSampler
    variable = 'disp_x disp_y'
    points = '0.0 0.0 0.0  0.5 0.5 0.0  1.0 1.0 0.0'
    sort_by = x
  [../]
[]
