#[derive(Debug, Clone)]
pub struct Table<T, const LEN: usize> {
    entries: Box<[Option<T>; LEN]>,
}

impl<T, const LEN: usize> Default for Table<T, LEN> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const LEN: usize> Table<T, LEN> {
    pub fn new() -> Self {
        let entries = Vec::from_iter(std::iter::from_fn(|| Some(None)).take(LEN))
            .try_into()
            .ok()
            .unwrap();

        Self { entries }
    }

    #[inline(always)]
    pub fn get(&self, index: usize) -> Option<&T> {
        self.entries.get(index).unwrap().as_ref()
    }

    #[inline(always)]
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.entries.get_mut(index).unwrap().as_mut()
    }

    #[inline(always)]
    pub fn insert(&mut self, index: usize, value: T) -> &mut T {
        let reference = self.entries.get_mut(index).unwrap();
        *reference = Some(value);
        reference.as_mut().unwrap()
    }

    #[inline(always)]
    pub fn remove(&mut self, index: usize) -> Option<T> {
        self.entries.get_mut(index).unwrap().take()
    }

    #[inline(always)]
    pub fn get_or_default(&mut self, index: usize) -> &mut T
    where
        T: Default,
    {
        let entry = self.entries.get_mut(index).unwrap();
        if entry.is_some() {
            let entry = self.entries.get_mut(index).unwrap().as_mut().unwrap();
            return entry;
        }

        self.insert(index, T::default())
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.entries.fill_with(|| None);
    }
}
