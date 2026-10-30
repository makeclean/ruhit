[Problem]
    # Specify coordinate system type
    coord_type = RZ
[]

[Mesh]
    file = pelletcladmergedfine10_rz.e
    displacements = 'disp_x disp_y'
    patch_size = 1000 # For contact algorithm
[]
[1]
    some = thing
    [./2]
        another=value
        sliceheight = ${/ ${fuelheight} ${numslices}}
    [../]
[]