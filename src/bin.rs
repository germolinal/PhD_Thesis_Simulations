use std::env;
use std::io::Write;

use std::fs;
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

#[allow(dead_code)]
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
fn add_wall_to_space(case: Case, building: &mut Building, state: &mut SimulationState, space_index : usize, wall_area: f64, window_area: f64, wall_construction_index: usize, window_construction_index: usize){
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
    if window_area > 0.0 {
        let window_polygon = get_squared_polygon(window_area, 0.0);

        let position = if case.has_control(){
            FenestrationPositions::Binary
        }else{
            FenestrationPositions::FixedClosed
        };
        
        let window_index = building.add_fenestration(state, format!("Window in space {}", space_name), position, FenestrationType::Window);
        building.set_fenestration_construction(window_index, window_construction_index).unwrap();     
        building.set_fenestration_polygon(window_index, window_polygon).unwrap();
        building.set_fenestration_front_boundary(window_index, Boundary::Space(space_index)).unwrap();
    }     

}


fn add_space(case: Case, building: &mut Building, state: &mut SimulationState, name: &str, length: f64, width: f64, height: f64, importance: f64) -> usize {
    // Volume
    let volume = length * width * height;
    let space_index = building.add_space(name.to_string());
    building.set_space_volume(space_index, volume).unwrap();

    let importance_schedule = Box::new(ScheduleConstant::new(importance));

    building.set_space_importance(space_index, importance_schedule).unwrap();

    
    if case.has_control(){
        // Heater
        building.add_heating_cooling_to_space(state,space_index, HeatingCoolingKind::ElectricHeating).unwrap();
        building.set_space_max_heating_power(space_index, 1500.).unwrap();
    
        // Lights
        building.add_luminaire_to_space(state, space_index).unwrap();
        building.set_space_max_lighting_power(space_index, 180.0).unwrap();
    }
    
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

fn create_building(case: Case, building: &mut Building, state: &mut SimulationState){
    // Set materials: All surfaces are made of 180mm concrete, except for windows.

    /* ************* */
    /* ADD MATERIALS */
    /* ************* */
    // Concrete
    let concrete_construction_index = add_construction(building, "Concrete", SubstanceProperties{
        thermal_conductivity: 2.33, // W/m.K            
        specific_heat_capacity: 960., // J/kg.K
        density: 2400., // kg/m3
    }, 180.0/1000.0); // 180mm
    
    
    // Glass
    let glass_construction_index = add_construction(building, "Glass", SubstanceProperties{
        thermal_conductivity: 2.33, // W/m.K            
        specific_heat_capacity: 960., // J/kg.K
        density: 2400., // kg/m3
    }, 3.0/1000.0); // 3mm
    
    /* ************ */
    /* ADD GEOMETRY */
    /* ************ */
    
    let building_height = 2.5; // m

    // 2B + Livingroom + Bathroom setup.
    let bed_1      = add_space(case, building, state, "Bedroom 1",  3.6, 4.0, building_height, 1.0);
    let bed_2      = add_space(case, building, state, "Bedroom 2",  2.4, 3.0, building_height, 1.0);
    let livingroom = add_space(case, building, state, "Living room", 4.6, 4.0, building_height, 1.0);
    let bathroom   = add_space(case, building, state, "Bathroom",   1.9, 2.4, building_height, 0.03);
    let kitchen    = add_space(case, building, state, "Kitchen",    2.4, 4.3, building_height, 0.1);
    let hallway    = add_space(case, building, state, "Hallway",    1.0, 4.3, building_height, 0.01);
    
    /* PERIMETER */

    // bedroom 1
    let wall_perimeter = (1557.0 + 4115. + 3891.)/1000.0;
    let wall_area = wall_perimeter * building_height;
    let window_perimeter = 1.7;
    let window_area = window_perimeter; // assume that windows are 1m hight
    add_wall_to_space(case, building, state, bed_1, wall_area, window_area, concrete_construction_index, glass_construction_index);

    // bedroom 2
    let wall_perimeter = (3000.0 + 2339.)/1000.0;
    let wall_area = wall_perimeter * building_height;
    let window_perimeter = 0.9;
    let window_area = window_perimeter; // assume that windows are 1m hight
    add_wall_to_space(case, building, state, bed_2, wall_area, window_area, concrete_construction_index, glass_construction_index);

    // livingroom
    let wall_perimeter = (4000.0 + 4300.)/1000.0;
    let wall_area = wall_perimeter * building_height;
    let window_perimeter = 2.3;
    let window_area = window_perimeter; // assume that windows are 1m hight
    add_wall_to_space(case, building, state, livingroom, wall_area, window_area, concrete_construction_index, glass_construction_index);

    // bathroom
    let wall_perimeter = 1.9;
    let wall_area = wall_perimeter * building_height;
    let window_perimeter = 0.5;
    let window_area = window_perimeter; // assume that windows are 1m hight
    add_wall_to_space(case, building, state, bathroom, wall_area, window_area, concrete_construction_index, glass_construction_index);
    /*
    */
    // kitchen
    let wall_perimeter = (2400.0 + 4200.)/1000.0;
    let wall_area = wall_perimeter * building_height;
    let window_perimeter = 1.3;
    let window_area = window_perimeter; // assume that windows are 1m hight
    add_wall_to_space(case, building, state, kitchen, wall_area, window_area, concrete_construction_index, glass_construction_index);

    // Hallway
    let wall_perimeter = 1.0;
    let wall_area = wall_perimeter * building_height;
    let window_perimeter = 0.9;
    let window_area = window_perimeter; // assume that windows are 1m hight
    add_wall_to_space(case, building, state, hallway, wall_area, window_area, concrete_construction_index, glass_construction_index);
    /*
    */

    /* CONNECTIONS BETWEEN ZONES */
    //add_wall_between_spaces(building, bed_1, hallway, 2.4 * building_height, concrete_construction_index);
    //add_wall_between_spaces(building, bed_1, livingroom, 4.0 * building_height, concrete_construction_index);
    
    //add_wall_between_spaces(building, livingroom, hallway, 1.9 * building_height, concrete_construction_index);
    //add_wall_between_spaces(building, livingroom, kitchen, 2.4 * building_height, concrete_construction_index);
    
    //add_wall_between_spaces(building, kitchen, hallway, 1.9 * building_height, concrete_construction_index);
    //add_wall_between_spaces(building, kitchen, bathroom, 2.3 * building_height, concrete_construction_index);
    
    //add_wall_between_spaces(building, bathroom, hallway, 1.9 * building_height, concrete_construction_index);
    //add_wall_between_spaces(building, bathroom, bed_2, 2.3 * building_height, concrete_construction_index);
    
    //add_wall_between_spaces(building, bed_2, hallway, (2.239 + 0.657) * building_height, concrete_construction_index);
    
    
}

#[derive(Copy,Clone)]
enum Case {
    Section1_1,
    Section1_2WithControl,
    Section1_2WithoutControl,
    Section1_3NotBusy,
    Section1_3Busy
}

impl Case {

    fn has_control(&self)->bool{
        match self {
            Case::Section1_1 => true,
            Case::Section1_2WithControl => true,
            Case::Section1_2WithoutControl => false,
            Case::Section1_3Busy => false,
            Case::Section1_3NotBusy => false
        }
    }

    fn is_busy(&self)->bool {
        match self{
            Case::Section1_1 => false,
            Case::Section1_2WithControl => false,
            Case::Section1_2WithoutControl => false,
            Case::Section1_3Busy => true,
            Case::Section1_3NotBusy => false
        }
    }

    fn is_proactive(&self)->bool{
        match self {
            Case::Section1_1 => true,
            Case::Section1_2WithControl => false,
            Case::Section1_2WithoutControl => false,
            Case::Section1_3Busy => true,
            Case::Section1_3NotBusy => true
        }
    }

    fn filename(&self)->&str{
        match self {
            Case::Section1_1 => "Section1_1",
            Case::Section1_2WithControl => "Section1_2WithControl",
            Case::Section1_2WithoutControl => "Section1_2WithoutControl",
            Case::Section1_3Busy => "Section1_3Busy",
            Case::Section1_3NotBusy => "Section1_3NotBusy"
        }
    }
}


fn write_operation( case: Case, building: &Building, data : serde_json::Value){
    
        
    let mut file = std::fs::File::create(format!("{}.csv",case.filename())).unwrap();

    let mut wrote_header = false;
    

    let data = data.as_array().unwrap();
    for tstep in data {
        let tstep = tstep.as_object().unwrap();
        let date = tstep.get("timestep_start").unwrap().as_object().unwrap();
        let date : Date = Date {
            month: date.get("month").unwrap().as_u64().unwrap() as usize,
            day: date.get("day").unwrap().as_u64().unwrap() as usize,
            hour: date.get("hour").unwrap().as_f64().unwrap(),
        };
        let controllers = tstep.get("controllers").unwrap();        
        let person = controllers.as_object().unwrap().get("person").unwrap().as_object().unwrap();
        
        /*
        if !wrote_header {
            let mut status_header = String::new();
            let status = person.get("current_status").unwrap().as_array().unwrap();
            for per in status {
                let per = per.as_object().unwrap();
                let space = per.get("space").unwrap().as_str().unwrap();
                let perception = per.get("perception").unwrap().as_str().unwrap();
                status_header += format!(";{} - {}", perception, space).as_str();
            }
            file.write_all(format!("Date;Comfort{};Perception to fix;Location of perception to fix;Actions taken\n", status_header).as_bytes()).unwrap();
            wrote_header = true;
        }
        */
        
        let attended = person.get("attended").unwrap().as_bool().unwrap();
        //let behaved = person.get("behaved").unwrap().as_bool().unwrap();
        if attended {            
            let perception_to_fix = person.get("perception_to_fix").unwrap().as_str().unwrap();
            let location_index = person.get("location_of_worst_perception").unwrap().as_i64().unwrap() as usize;
            let location_to_fix = building.get_space(location_index).unwrap().name();
            
            let actions_taken_vec = person.get("actions_taken").unwrap().as_array().unwrap();
            let mut actions_taken : String = "".to_string();
            
            let comfort = person.get("potential_comfort").unwrap().as_f64().unwrap();

            // Register perceptions
            let status = person.get("current_status").unwrap().as_array().unwrap();
            let mut status_values = String::new();
            for per in status {
                let per = per.as_object().unwrap();
                let val = per.get("value").unwrap().as_f64().unwrap();
                status_values+=format!(";{}",val).as_str();
            }

            // Register actions
            if actions_taken_vec.is_empty(){
               actions_taken = "None".to_string(); 
            }else{
                for v in actions_taken_vec {
                    let v = v.as_array().unwrap();
                    assert_eq!(v.len(), 2);
                    let action = v[0].as_str().unwrap();
                    let loc = v[1].as_str().unwrap();
                    actions_taken = format!("{}{}{} in {} ",actions_taken, if actions_taken.is_empty() {""}else{", "}, action, loc);                    
                }
            }


            


            file.write_all(format!("{};{}{};{};{};{}\n", date,comfort,status_values, perception_to_fix, location_to_fix, actions_taken).as_bytes()).unwrap();
        }

    }

}

fn write_comfort( case: Case, data : serde_json::Value){
    
        
    let mut file = std::fs::File::create(format!("{}.csv",case.filename())).unwrap();
    file.write_all("Date,ActualComfort,PotentialComfort,Satisfaction\n".as_bytes()).unwrap();

    let data = data.as_array().unwrap();
    for tstep in data {
        let tstep = tstep.as_object().unwrap();
        let date = tstep.get("timestep_start").unwrap().as_object().unwrap();
        let date : Date = Date {
            month: date.get("month").unwrap().as_u64().unwrap() as usize,
            day: date.get("day").unwrap().as_u64().unwrap() as usize,
            hour: date.get("hour").unwrap().as_f64().unwrap(),
        };
        let controllers = tstep.get("controllers").unwrap();        
        let person = controllers.as_object().unwrap().get("person").unwrap().as_object().unwrap();
        
        let attended = person.get("attended").unwrap().as_bool().unwrap();
        if attended {            
            let actual_comfort = person.get("current_comfort").unwrap().as_f64().unwrap();
            let potential_comfort = person.get("potential_comfort").unwrap().as_f64().unwrap();
            let satisfaction = person.get("dwelling_satisfaction_before").unwrap().as_f64().unwrap();

            file.write_all(format!("{},{},{},{}\n", date, actual_comfort, potential_comfort, satisfaction).as_bytes()).unwrap();
        }

    }

}

fn main() {
    
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        println!("Error... Usage is: {} weather_file case", args[0]);
        return;
    }

    let case = if args[2] == "case1" {
        Case::Section1_1
    }else if args[2] == "case2_without_control" {
        Case::Section1_2WithoutControl
    }else if args[2] == "case2_with_control" {
        Case::Section1_2WithControl
    }else if args[2] == "case3_busy" {
        Case::Section1_3Busy
    }else if args[2] == "case3_not_busy" {
        Case::Section1_3NotBusy
    }else { 
        panic!("Unknown case '{}'", args[2]);
    };

    /* ****************** */
    /* CREATE MAIN ACTORS */
    /* ****************** */

    let mut state = SimulationState::new();        
    let mut building = Building::new("The Building".to_string()); 
    let mut person = if case.has_control() {
        Person::new(&mut state)
    }else{
        Person::with_fixed_clothing(&mut state, 1.0)        
    };
    if case.is_busy(){
        person.set_sleeping_hours(22.5, 6.5);
    }


    

    
    /* ***************** */
    /* LOAD WEATHER FILE */
    /* ***************** */    
    let weather_file_name = args[1].clone();
    let weather = EPWWeather::from_file(weather_file_name);    

    /* ***************** */
    /*   DEFINE PERSON   */
    /* ***************** */
    
    // Constant proactivity.
    
    let proactivity = if case.is_proactive(){
        0.99
    }else{
        0.0
    };
    person.set_proactivity(Box::new(ScheduleConstant::new(proactivity))).unwrap();

    // Constant busyness    
    let busyness = if case.is_busy(){
        24.0
    }else{
        0.0
    }; 
    person.set_busyness(Box::new(ScheduleConstant::new(busyness))).unwrap();

    // Constant awareness of the future, 3 hours
    let awareness = ScheduleConstant::new(3. * 3600.);
    person.set_awareness_of_the_future(Box::new(awareness)).unwrap();
    
    // Add perceptions that are relevant to the person. These are polynomials 
    // representing how different perceptions affect the person's immediate 
    // satisfaction with the space. These are arbitrary (for now) and they
    // only respect the signs (e.g. good vs bad percepcions)

    // Cold and hot thermal sensations are equally bad -> 0 + 0*x - 2*x^2
    person.add_perception( poly![0.0, 0.0, -2.], Perception::ThermalSensationCold);    
    person.add_perception( poly![0.0, 0.0, -2.], Perception::ThermalSensationHot);    

    // Too much and too little clothing are equally bad -> 0 + 0*x - 1*x^2
    person.add_perception( poly![0.0, 0.0, -1.5], Perception::ClothingAnnoyanceTooMuch);
    person.add_perception( poly![0.0, 0.0, -1.5], Perception::ClothingAnnoyanceTooLittle);    

    // Too much and too little Loudness are equally bad -> 0 + 0*x - 1*x^2
    person.add_perception( poly![0.0, 0.0, -2.], Perception::LoudnessTooMuch);
    person.add_perception( poly![0.0, 0.0, -2.], Perception::LoudnessTooLittle);

    // Brightness is good (more is better) -> 0 + 7*x
    person.add_perception( poly![0.0, 5.0], Perception::Brightness);

    // Utility bills are bad... -> 0 -6*x^2
    person.add_perception( poly![0.0, 0.0, -0.1], Perception::UtilityBills);

    

    /* ***************** */
    /*  DEFINE BUILDING  */
    /* ***************** */

    // For the sake of clarity and briefness, this is summarized
    // in this way. You can find the whole program at 
    // https://github.com/germolinal/PhD_Thesis_Simulations
    //
    // This function defines a 2-Bedroom + Livingroom + Bathroom + Kitchen 
    // + Hallway home. All walls are made of 180mm concrete, and the windows are
    // 3mm glass.
    // 
    // Every space has openable windows, a 1500W heater and 180W of 
    // switchable lights
    create_building(case, &mut building, &mut state);

    
    /* ******************** */
    /*  DEFINE SIM. PERIOD  */
    /* ******************** */

    let start = Date{
        day: 1,
        month: 7,
        hour: 0.0,
    };

    let mut end = start.clone();    
    end.add_days(2);  

    /* ********** */
    /*  SIMULATE  */
    /* ********** */

    let n = 60; // tsteps per hour
    
    // This function is not publicly available, at least for now. Contact 
    // me for details.
    let results = simple_lib::run(start, end, &person, &mut building, &mut state, &weather, n).unwrap();

    /* *************** */
    /*  PRINT RESULTS  */
    /* *************** */
    
    let mut file = std::fs::File::create(format!("{}.json",case.filename())).unwrap();
    let file_content = format!("{}",serde_json::to_string_pretty(&results).unwrap());
    file.write_all(file_content.as_bytes()).unwrap();
    
    /* PROCESS RESULTS */

    let data = fs::read_to_string(format!("{}.json",case.filename())).unwrap();
    let res : serde_json::Value = serde_json::from_str(&data).expect("Unable to parse");
    match case {
        Case::Section1_1 => {
            write_operation(case, &building, res);
        },
        _ => {
            write_comfort(case, res);
        }
    }
    
    
}
