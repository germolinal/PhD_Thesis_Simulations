use std::collections::HashMap;

use simulation_state::simulation_state::SimulationState;
use building_model::building::Building;
use communication_protocols::simulation_model::SimulationModel;
use calendar::date_factory::DateFactory;
use calendar::date::Date;
use people::people::People;
use multiphysics_model::multiphysics_model::MultiphysicsModel;
use weather::Weather;
use simple_results::{SimulationResults, TimeStepResults};



/// This function drives the simulation, after having parsed and built
/// the Building, State and Peoeple.
pub fn run(start: Date, end: Date, person: &dyn People, building: &mut Building, state: &mut SimulationState, weather: &dyn Weather, n: usize)->Result<SimulationResults,String>{
    
    
    if start == end || start.is_later(end) {
        return Err(format!("Time period inconsistency... Start = {} | End = {}", start, end));
    }
    
    let model = match MultiphysicsModel::new(&building, state, n){
        Ok(v)=>v,
        Err(e)=>return Err(e),
    };    

    // Map the state
    building.map_simulation_state(state)?;
    
    
    // Calculate timestep and build the Simulation Period
    let dt = 60. * 60. / n as f64;
    let sim_period = DateFactory::new(start, end, dt);
        
    // TODO: Calculate the capacity needed for the results
    let mut results = SimulationResults::new();
    
    
    // Simulate the whole simulation period
    for date in sim_period {    
        
        // initialize results struct
        let mut step_results = TimeStepResults{
            timestep_start : date,    
            state_elements : state.elements().clone(),
            weather : weather.get_weather_data(date),
            controllers: HashMap::new() 
        };
        
        // Get the current weather data
        //let current_weather = weather.get_weather_data(date);
        
        // Make the model march
        match model.march(date, weather, building, state ) {
            Ok(_)=>{},
            Err(e) => panic!(e)
        }
        
        
        // Control the building or person, if needed        
        let person_result = person.control(date, weather, building, &model, state);
        step_results.controllers.insert(format!("person"), person_result);

        // push results
        results.push(step_results);        
    }
    
    Ok(results)
    
}



