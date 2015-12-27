#![macro_use]

macro_rules! err {
	{
		$name:ident ;
		$( $v:ident($t:ty) ),*
	} => {
		#[derive(Debug)]
		enum $name {
			$( $v($t) ),*
		}

		$(
		impl ::std::convert::From<$t> for $name {
			fn from(err: $t) -> $name {
				$name::$v(err)
			}
		}
		)*
	}
}
