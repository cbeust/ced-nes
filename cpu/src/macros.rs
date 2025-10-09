#[macro_export]
macro_rules! word {
($a: expr, $b: expr)=> {
        (($b as u16) << 8) | ($a as u16)
    }
}

#[macro_export]
macro_rules! zp {
    ($pc: expr, $memory: expr)=> {
        $memory.get($pc.wrapping_add(1)) as u16
    }
}

#[macro_export]
macro_rules! zp_x {
    ($pc: expr, $memory: expr, $x: expr)=> {
        (($memory.get($pc.wrapping_add(1)) as u16 + $x as u16) as u8) as u16
    }
}

#[macro_export]
macro_rules! abs {
    ($pc: expr, $memory: expr)=> {
        crate::word!($memory.get($pc.wrapping_add(1)), $memory.get($pc.wrapping_add(2)))
    }
}

#[macro_export]
macro_rules! abs_d {
    ($pc: expr, $memory: expr)=> {
        crate::word!($memory.get_direct($pc.wrapping_add(1)), $memory.get_direct($pc.wrapping_add(2)))
    }
}

#[macro_export]
macro_rules! abs_value_d {
    ($pc: expr, $memory: expr)=> {
        {
            let a = crate::word!($memory.get($pc.wrapping_add(1)), $memory.get_direct($pc.wrapping_add(2)));
            let v = $memory.get_direct(a);
            (a, v)
        }
    }
}

#[macro_export]
macro_rules! abs_x {
    ($pc: expr, $memory: expr, $x: expr)=> {
        crate::word!($memory.get($pc.wrapping_add(1)), $memory.get($pc.wrapping_add(2))) + $x as u16
    }
}

#[macro_export]
macro_rules! abs_y {
    ($pc: expr, $memory: expr, $y: expr)=> {
        crate::word!($memory.get($pc.wrapping_add(1)), $memory.get($pc.wrapping_add(2))) + $y as u16
    }
}

#[macro_export]
macro_rules! ind_y {
    ($pc: expr, $memory: expr, $y: expr)=> {
        {
            let address = $memory.get($pc.wrapping_add(1));
            let next2 = if address == 0xFF { 0 } else { address.wrapping_add(1) };
            crate::word!($memory.get(address as u16), $memory.get(next2 as u16)).wrapping_add($y as u16)
        }
    }
}

#[macro_export]
macro_rules! ind_x {
    ($pc: expr, $memory: expr, $x: expr)=> {
        {
            let next = $pc.wrapping_add(1);
            let v0 = $memory.get(next);
            let byte0 = $memory.get((v0 as u16 + $x as u16) & 0xff);
            let byte1 = $memory.get((v0 as u16 + $x.wrapping_add(1) as u16) & 0xff);
            crate::word!(byte0, byte1)
        }
    }
}
