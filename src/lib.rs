use std::collections::{HashMap, HashSet};
use std::cmp::{max,min};
use rand::Rng;
use std::hash::Hash;

pub struct MultiMarkovModel<T: Eq + Hash + Clone + Copy> {
    pub frequencies: HashMap<Vec<T>,HashMap<T,f64>>,
    pub known_states: HashSet<T>,
    order: i32,
    // TODO: add a random number generator (or seed?) that the user can specify, or go with a default
}
impl<T: Eq + Hash + Clone + Copy> MultiMarkovModel<T> {

    pub const DEFAULT_ORDER: i32 = 3;
    pub const DEFAULT_PRIOR: f64 = 0.005;

    pub fn new() -> MultiMarkovModel<T> {
        MultiMarkovModel {
            frequencies: HashMap::new(),
            known_states: HashSet::new(),
            order: MultiMarkovModel::<T>::DEFAULT_ORDER, // TODO: confirm: is this immutable once set? it should be, so we don't train and retrieve with different assumed orders
        }
    }

    // TODO: an overloaded "train" method that handles all data ingestion and sets priors; optionally setting a custom order and custom prior

    /// Takes in a vector of sequences, and calls the `add_sequence` method on
    /// each one in turn, training the model.
    ///
    /// ```
    /// use multimarkov::MultiMarkovModel;
    /// let mut model = MultiMarkovModel::new();
    /// let input_vec = vec![
    ///     vec!['a'],
    ///     vec!['f','o','o','b','a','r'],
    ///     vec!['b','a','z'],
    /// ];
    /// assert!(model.add_sequences(input_vec).is_ok()); // assert short value "a" did not abort training
    /// assert!(model.frequencies.contains_key(&*vec!['b']));
    /// assert_eq!(*model.frequencies.get(&*vec!['b']).unwrap().get(&'a').unwrap(),2.0); // both sequences contain 'b' -> 'a' once
    /// ```
    /// TODO: take an iterator directly instead of a vector
    pub fn add_sequences(&mut self, sequences: Vec<Vec<T>>) -> Result<(), &'static str> {
        if sequences.len() < 1 { return Err("no sequences in input"); }
        for sequence in sequences {
            match self.add_sequence(sequence) {
                Ok(()) => (),
                Err(e) => {
                    println!("error ignored: {}",e);
                }
            };
        }
        return Ok(());
    }

    /// Adds to the model all the observed state transitions found in one sequence of training data.
    /// This training is additive; it doesn't empty or overwrite the model, so you can call this
    /// method on many such training sequences in order to fully train the model.
    ///
    /// ```
    /// use multimarkov::MultiMarkovModel;
    /// let mut model = MultiMarkovModel::new();
    /// model.add_sequence(vec!['h','e','l','l','o']);
    /// assert!(model.frequencies.contains_key(&*vec!['l']));
    /// assert!(model.frequencies.contains_key(&*vec!['l','l']));
    /// assert!(model.frequencies.get(&*vec!['l']).unwrap().contains_key(&'l'));
    /// assert!(model.frequencies.get(&*vec!['l','l']).unwrap().contains_key(&'o'));
    /// ```
    pub fn add_sequence(&mut self, sequence: Vec<T>) -> Result<(), String> {
        if sequence.len() < 2 { return Err(format!("sequence was too short, must contain at least two states")); }

        // loop backwards through the characters in the sequence
        for i in (1..sequence.len()).rev() {
            // Build a running set of all known characters while we're at it
            self.known_states.insert(sequence[i]);
            // For the sequences preceding character (i), record that character (i) was observed following them.
            // IE if the char_vec is ['R','U','S','T'] and this is a 3rd-order model, then for the three models ['S'], ['U','S'], and ['R','U','S'] we record that ['T'] is a known follower.
            for j in (max(0,i as i32 - self.order) as usize)..i {
                *self.frequencies.entry(Vec::from(&sequence[j..i])).or_insert(HashMap::new()).entry(sequence[i]).or_insert(0.0) += 1.0;
            }
        }
        self.known_states.insert(sequence[0]); // previous loop stops before index 0
        Ok(())
    }

    /// Fills in missing state transitions with a given value so that any observed state (except
    /// those only seen at the end of sequences) can transition to any other state.
    ///
    /// ```
    /// use multimarkov::MultiMarkovModel;
    /// let mut model = MultiMarkovModel::new();
    /// model.add_sequence(vec!['a','b','c']);
    /// model.add_priors(MultiMarkovModel::<char>::DEFAULT_PRIOR);
    /// assert_eq!(*model.frequencies.get(&*vec!['a']).unwrap().get(&'b').unwrap(),1.0); // learned from training data
    /// assert_eq!(*model.frequencies.get(&*vec!['b']).unwrap().get(&'a').unwrap(),0.005); // not observed in training data; set to DEFAULT_PRIOR by add_priors
    /// ```
    pub fn add_priors(&mut self, prior: f64) {
        for v in self.frequencies.values_mut() {
            for &a in self.known_states.iter() {
                v.entry(a).or_insert(prior);
            }
        }
    }

    /// Using the random-number generator and the "weights" of the various state transitions from
    /// the trained model, draw a new state to follow the given sequence.
    pub fn random_next(&mut self, current_sequence: &Vec<T>) -> Option<T> {
        let bestmodel = self.best_model(current_sequence)?;
        let sum_of_weights: f64 = bestmodel.values().sum();
        // TODO: use an RNG or RNG seed stored in the struct, so the user can specify it if desired
        let mut rng = rand::thread_rng();
        let r: f64 = rng.gen();
        let mut randomroll = r*sum_of_weights; // TODO: can this be accomplished in fewer lines?
        // every state has a chance of being selected in proportion to its 'weight' as fraction of the sum of weights
        for (k,v) in bestmodel {
            if randomroll > *v {
                randomroll -= v;
            } else {
                return Some(k.clone());
            }
        }
        None // this should never be reached
    }

    /// For a given sequence, find the most tightly-fitted model we have for its tail-end subsequence.
    /// For example, if the sequence is `['t','r','u','s']`, and self.order==3, first see if we have
    /// a model for `['r','u','s']`, which will only exist if that sequence has been seen in the training
    /// data.  If not, see if we have a model for `['u','s']`, and failing that, see if we have a
    /// model for `['s']`.  If no model for `['s']` is found, return `None`.
    ///
    /// ```
    /// use multimarkov::MultiMarkovModel;
    /// let mut model = MultiMarkovModel::new();
    /// let input_vec = vec![
    ///     vec!['a','c','e'],
    ///     vec!['f','o','o','b','a','r'],
    ///     vec!['b','a','z'],
    /// ];
    /// model.add_sequences(input_vec);
    /// let bestmodel = model.best_model(&vec!['b','a']).unwrap();
    /// assert!(bestmodel.contains_key(&'r')); // 'r' follows ['a'] as well as ['b','a']
    /// assert!(!bestmodel.contains_key(&'c')); // 'c' follows ['a'], but doesn't follow ['b','a']
    /// ```
    pub fn best_model(&self, current_sequence: &Vec<T>) ->  Option<&HashMap<T,f64>> {
        // If current_sequence.len() is at least self.order, count "i" down from self.order to 1,
        // taking sequence slices of length "i" and checking if we have a matching model:
        for i in (1..(min(self.order as usize, current_sequence.len())+1)).rev() {
            let subsequence = &current_sequence[(current_sequence.len()-i)..current_sequence.len()];
            if self.frequencies.contains_key(subsequence) {
                return self.frequencies.get(subsequence);
            }
        }
        None
    }


}