//! A gate: routes of conditions. Any one route whose conditions all hold opens it.

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Gate<C> {
    routes: Vec<Vec<C>>,
}

impl<C> Gate<C> {
    /// No routes never opens; an empty route always does.
    pub fn new(routes: Vec<Vec<C>>) -> Self {
        Self { routes }
    }

    /// The one-condition case: opens for whoever holds `condition`.
    pub fn single(condition: C) -> Self {
        Self::new(vec![vec![condition]])
    }

    /// Opens for everyone.
    pub fn open() -> Self {
        Self::new(vec![Vec::new()])
    }

    pub fn routes(&self) -> &[Vec<C>] {
        &self.routes
    }

    pub fn opens(&self, holds: impl Fn(&C) -> bool) -> bool {
        self.routes.iter().any(|route| route.iter().all(&holds))
    }

    /// The closest route's missing conditions: fewest missing, first route on a
    /// tie. Empty when the gate opens; `None` when it has no routes.
    pub fn missing(&self, holds: impl Fn(&C) -> bool) -> Option<Vec<&C>> {
        self.routes
            .iter()
            .map(|route| route.iter().filter(|c| !holds(c)).collect::<Vec<_>>())
            .min_by_key(Vec::len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held(have: &[u8]) -> impl Fn(&u8) -> bool + '_ {
        move |c| have.contains(c)
    }

    #[test]
    fn any_one_complete_route_opens_it() {
        let g = Gate::new(vec![vec![1, 2], vec![3]]);
        assert!(g.opens(held(&[3])));
        assert!(g.opens(held(&[1, 2])));
        assert!(!g.opens(held(&[1])), "half a route is no route");
        assert!(!g.opens(held(&[2])));
        assert!(!g.opens(held(&[])));
    }

    #[test]
    fn no_routes_never_opens_and_an_empty_route_always_does() {
        assert!(!Gate::<u8>::new(vec![]).opens(|_| true));
        assert_eq!(Gate::<u8>::new(vec![]).missing(|_| true), None);
        assert!(Gate::<u8>::open().opens(|_| false));
    }

    #[test]
    fn single_is_the_one_condition_case() {
        let g = Gate::single(4u8);
        assert!(g.opens(held(&[4])));
        assert!(!g.opens(held(&[5])));
        assert_eq!(g.missing(held(&[5])), Some(vec![&4]));
    }

    #[test]
    fn missing_reports_the_closest_route() {
        let g = Gate::new(vec![vec![1, 2, 3], vec![4, 5], vec![6, 7]]);
        assert_eq!(
            g.missing(held(&[1, 2])),
            Some(vec![&3]),
            "one short beats two"
        );
        assert_eq!(
            g.missing(held(&[])),
            Some(vec![&4, &5]),
            "first route on a tie"
        );
        assert_eq!(g.missing(held(&[7])), Some(vec![&6]));
        assert_eq!(
            g.missing(held(&[6, 7])),
            Some(vec![]),
            "open: nothing missing"
        );
    }
}
