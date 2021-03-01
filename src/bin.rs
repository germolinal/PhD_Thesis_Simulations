use std::env;

use serde_json;        



extern crate simple_lib;
use simulation_state::simulation_state::SimulationState;
use calendar::date::Date;

use weather::epw_weather::EPWWeather;
use people::person::Person;
use people::perceptions::Perception;
use people::people::People;
use schedule::constant::ScheduleConstant;
use polynomial::*;

use building_model::building::Building;
use building_model::material::MaterialProperties;
use building_model::substance::SubstanceProperties;
use building_model::object_trait::ObjectTrait;
use building_model::boundary::Boundary;
use building_model::fenestration::{FenestrationPositions,FenestrationType};
use building_model::heating_cooling::HeatingCoolingKind;

use geometry3d::loop3d::Loop3D;
use geometry3d::point3d::Point3D;
use geometry3d::polygon3d::Polygon3D;

fn get_squared_polygon(outer_area: f64, inner_area: f64)->Polygon3D{
    assert!(outer_area > inner_area);

    // Create the outer part
    let mut the_loop = Loop3D::new();
    let l = outer_area.sqrt();

    the_loop.push( Point3D::new(-l, -l, 0.)).unwrap();
    the_loop.push( Point3D::new(l, -l, 0.)).unwrap();
    the_loop.push( Point3D::new(l, l, 0.)).unwrap();
    the_loop.push( Point3D::new(-l, l, 0.)).unwrap();
    the_loop.close().unwrap();
    
    let mut p = Polygon3D::new(the_loop).unwrap();

    if inner_area > 0.0 {

        let l = inner_area.sqrt();
        let mut the_inner_loop = Loop3D::new();
        the_inner_loop.push( Point3D::new(-l, -l, 0.)).unwrap();
        the_inner_loop.push( Point3D::new(l, -l, 0.)).unwrap();
        the_inner_loop.push( Point3D::new(l, l, 0.)).unwrap();
        the_inner_loop.push( Point3D::new(-l, l, 0.)).unwrap();
        the_inner_loop.close().unwrap();
        p.cut_hole(the_inner_loop.clone()).unwrap();
    }
    p
}

fn add_wall_between_spaces(building: &mut Building, space_a_index: usize, space_b_index: usize, area: f64, wall_construction_index: usize){
    let space_a_name: String;
    let space_b_name: String;
    {
        let space_a = building.get_space(space_a_index).unwrap();
        space_a_name = space_a.name().clone();
        let space_b = building.get_space(space_b_index).unwrap();
        space_b_name = space_b.name().clone();
    }

    // Square with no windows
    let p = get_squared_polygon(area, 0.0);

    // Add surface
    let surface_index = building.add_surface(format!("Surface between Spaces {} and {}", space_a_name, space_b_name));
    building.set_surface_construction(surface_index,wall_construction_index).unwrap();
    building.set_surface_polygon(surface_index, p).unwrap();
    
    building.set_surface_front_boundary(surface_index, Boundary::Space(space_a_index)).unwrap();
    building.set_surface_back_boundary(surface_index, Boundary::Space(space_b_index)).unwrap();

}

/// Adds a wall to a space... can have a window.
fn add_wall_to_space(building: &mut Building, state: &mut SimulationState, space_index : usize, wall_area: f64, window_area: f64, wall_construction_index: usize, window_construction_index: usize){
    assert!(wall_area > window_area);

    let space_name : String;
    {
        let space = building.get_space(space_index).unwrap();        
        space_name = space.name().clone();
    }

    let p = get_squared_polygon(wall_area, window_area);

    // Add surface
    let surface_index = building.add_surface(format!("Outer Surface {}", space_name));
    building.set_surface_construction(surface_index,wall_construction_index).unwrap();
    building.set_surface_polygon(surface_index, p).unwrap();
    
    building.set_surface_front_boundary(surface_index, Boundary::Space(space_index)).unwrap();

    // Add window.        
    let window_polygon = get_squared_polygon(window_area, 0.0);
    let window_index = building.add_fenestration(state, format!("Window in space {}", space_name), FenestrationPositions::Binary, FenestrationType::Window);
    building.set_fenestration_construction(window_index, window_construction_index).unwrap();     
    building.set_fenestration_polygon(window_index, window_polygon).unwrap();
    building.set_fenestration_front_boundary(surface_index, Boundary::Space(space_index)).unwrap();

}


fn add_space(building: &mut Building, state: &mut SimulationState, name: &str, length: f64, width: f64, height: f64) -> usize {
    // Volume
    let volume = length * width * height;
    let space_index = building.add_space(name.to_string());
    building.set_space_volume(space_index, volume).unwrap();

    // Heater
    building.add_heating_cooling_to_space(state,0, HeatingCoolingKind::ElectricHeating).unwrap();
    building.set_space_max_heating_power(0, 1500.).unwrap();

    // Lights
    building.add_luminaire_to_space(state, 0).unwrap();
    building.set_space_max_lighting_power(0, 180.0).unwrap();
    
    // Return space index
    space_index
}

fn add_construction(building: &mut Building, substance_name: &'static str, properties: SubstanceProperties, thickness: f64)->usize{

    let substance_index = building.add_substance(substance_name.to_string());

    building.set_substance_properties(substance_index, properties).unwrap();

    // add material
    let mat_index = building.add_material(format!("{}mm {}", thickness*1000.0, substance_name.to_string()));
    building.set_material_properties(mat_index, MaterialProperties{
        thickness: thickness
    }).unwrap();

    building.set_material_substance(mat_index, substance_index).unwrap();

    // Add construction
    let construction_index = building.add_construction(format!("{} construction", substance_name));
    building.add_material_to_construction(construction_index, mat_index).unwrap();

    construction_index
    
}

fn create_building(building: &mut Building, state: &mut SimulationState){
    // Set materials: All surfaces are made of 180mm concrete, except for windows.

    /* ************* */
    /* ADD MATERIALS */
    /* ************* */
    // Concrete
    let concrete_construction_index = add_construction(building, "Concrete", SubstanceProperties{
        thermal_conductivity: 2.33, // W/m.K            
        specific_heat_capacity: 960., // J/kg.K
        density: 2400., // kg/m3
    }, 180.0/1000.0);
    
    
    // Glass
    let glass_construction_index = add_construction(building, "Glass", SubstanceProperties{
        thermal_conductivity: 2.33, // W/m.K            
        specific_heat_capacity: 960., // J/kg.K
        density: 2400., // kg/m3
    }, 3.0/1000.0);
    
    /* ************ */
    /* ADD GEOMETRY */
    /* ************ */
    
    let building_height = 2.5; // m

    // 2B + Livingroom + Bathroom setup.
    let bed_1      = add_space(building, state, "Bedroom 1",  4.0, 4.0, building_height);
    let bed_2      = add_space(building, state, "Bedroom 2",  4.0, 4.0, building_height);
    let livingroom = add_space(building, state, "Livingroom", 4.0, 4.0, building_height);
    let bathroom   = add_space(building, state, "Bathroom",   4.0, 4.0, building_height);
    
    // Perimeter
    add_wall_to_space(building, state, bed_1, 4.0*building_height, 0.5*4.0*building_height, concrete_construction_index, glass_construction_index);
    add_wall_to_space(building, state, bed_2, 4.0*building_height, 0.5*4.0*building_height, concrete_construction_index, glass_construction_index);
    add_wall_to_space(building, state, livingroom, 4.0*building_height, 0.5*4.0*building_height, concrete_construction_index, glass_construction_index);
    add_wall_to_space(building, state, bathroom, 4.0*building_height, 0.5*4.0*building_height, concrete_construction_index, glass_construction_index);

    // Connection between zones
    add_wall_between_spaces(building, bed_1, bed_2, 3.0 * building_height, concrete_construction_index);

    

    
}



fn main() {
    
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Error... Usage is: {} epw_file", args[0]);
        return;
    }

    /* ****************** */
    /* CREATE MAIN ACTORS */
    /* ****************** */

    let mut state = SimulationState::new();        
    let mut building = Building::new("The Building".to_string()); 
    let mut person = Person::new(&mut state);

    
    /* ***************** */
    /* LOAD WEATHER FILE */
    /* ***************** */
    let weather_file = args[1].clone();
    let weather = EPWWeather::from_file(weather_file);    

    /* ***************** */
    /*   DEFINE PERSON   */
    /* ***************** */
    
    // Constant proactivity.
    let proactivity = ScheduleConstant::new(0.4);
    person.set_proactivity(Box::new(proactivity)).unwrap();

    // Constant busyness
    let busyness = ScheduleConstant::new(0.2);
    person.set_busyness(Box::new(busyness)).unwrap();

    // Constant awareness of the future, 6 hours
    let awareness = ScheduleConstant::new(6.);
    person.set_awareness_of_the_future(Box::new(awareness)).unwrap();
    
    // Add perceptions that are relevant to the person. These are polynomials 
    // representing how different perceptions affect the person's immediate 
    // satisfaction with the space. These are arbitrary (for now) and they
    // only respect the signs (e.g. good vs bad percepcions)

    // Cold and hot thermal sensations are equally bad -> 0 + 0*x - 2*x^2
    person.add_perception( poly![0.0, 0.0, -2.], Perception::ThermalSensationCold);    
    person.add_perception( poly![0.0, 0.0, -2.], Perception::ThermalSensationHot);    

    // Too much and too little clothing are equally bad -> 0 + 0*x - 1*x^2
    person.add_perception( poly![0.0, 0.0, -1.], Perception::ClothingAnnoyanceTooMuch);
    person.add_perception( poly![0.0, 0.0, -1.], Perception::ClothingAnnoyanceTooLittle);    

    // Too much and too little Loudness are equally bad -> 0 + 0*x - 1*x^2
    person.add_perception( poly![0.0, 0.0, -1.], Perception::LoudnessTooMuch);
    person.add_perception( poly![0.0, 0.0, -1.], Perception::LoudnessTooLittle);

    // Brightness is good (more is better) -> 0 + 7*x
    person.add_perception( poly![0.0, 7.0], Perception::Brightness);

    // Utility bills are bad... -> 0 -6*x
    person.add_perception( poly![0.0, 0.0, -6.], Perception::UtilityBills);

    

    /* ***************** */
    /*  DEFINE BUILDING  */
    /* ***************** */

    // For the sake of clarity and briefness, this is summarized
    // in this way. 
    //
    // This function defines a 2-Bedroom + Livingroom + Bathroom
    // home. All walls are made of 180mm concrete, and the windows are
    // 3mm glass.
    // 
    // Every space has openable windows, a 1500W heater and 180W of 
    // switchable lights
    create_building(&mut building, &mut state);

    
    /* ******************** */
    /*  DEFINE SIM. PERIOD  */
    /* ******************** */

    let start = Date{
        day: 1,
        month: 1,
        hour: 0.0,
    };

    let mut end = start.clone();
    end.add_hours(72.0);

    /* ********** */
    /*  SIMULATE  */
    /* ********** */

    let n = 12; // tsteps per hour
    let results = simple_lib::run(start, end, &person, &mut building, &mut state, &weather, n).unwrap();

    /* *************** */
    /*  PRINT RESULTS  */
    /* *************** */

    println!("{}",serde_json::to_string_pretty(&results).unwrap())
    
    
}
