use ellocopo::Error;
use ellocopo::OperationStatus;
use core::convert::TryFrom;

pub const PROTOCOL_SIGN: u8 = 0x8E;

type ParseResult<'a, T> = Result<(&'a [u8], T), Error>;

/// Available register's types
#[allow(non_camel_case_types)]
#[derive(Clone, Debug)]
pub enum RegisterValue {
    UNIT,
    I32(i32),
    I16(i16),
    I8(i8),
    U32(u32),
    U16(u16),
    U8(u8),
    STR(String),
    BYTES(Vec<u8>),
}

#[repr(u8)]
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug)]
pub enum RegTypeId  {
    UNIT = 0,  
    I32 = 1, 
    I16 = 2,
    I8 = 3,
    U32 = 4,
    U16 = 5,
    U8 = 6,
    STR = 7,
    BYTES = 8,    
}

impl TryFrom<u8> for RegTypeId {
    type Error = ();
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        use RegTypeId::*;
        match value {
            0 => Ok(UNIT),
            1 => Ok(I32),
            2 => Ok(I16),
            3 => Ok(I8),
            4 => Ok(U32),
            5 => Ok(U16),
            6 => Ok(U8),
            7 => Ok(STR),
            8 => Ok(BYTES),
            _ => Err(()),
        }
    }
}

pub fn pars_answer(buf : &[u8]) -> Result<RegisterValue, Error> {
    use core::mem::transmute;
    use core::str::from_utf8_unchecked;

    let buf = &buf[..];
    let (buf, _) = sign_parser(buf)?;
    let (buf, op) = op_parser(buf)?;
    let (buf, name_sz) = name_sz_parser(buf)?;
    if buf.len() < name_sz { return Err(Error::BadFormat) }
    let (buf, name) = name_parser(buf, name_sz)?;
    use ellocopo::OperationStatus::*;
    match op {
        Read => {
            use RegTypeId::*;
            let (buf, rtype) = value_type(buf)?;
            match rtype{
                U32 => {
                    let (_, (_type_id, value_bytes)) = value_parser(buf, rtype)?;
                    let value: &u32 = unsafe { transmute(value_bytes.as_ptr()) };
                    Ok(RegisterValue::U32(*value))
                }
                U16 => {
                    let (_, (_type_id, value_bytes)) = value_parser(buf, rtype)?;
                    let value: &u16 = unsafe { transmute(value_bytes.as_ptr()) };
                    Ok(RegisterValue::U16(*value))
                }
                U8 => {
                    let (_, (_type_id, value_bytes)) = value_parser(buf, rtype)?;
                    let value: &u8 = unsafe { transmute(value_bytes.as_ptr()) };
                    Ok(RegisterValue::U8(*value))
                }
                I32 => {
                    let (_, (_type_id, value_bytes)) = value_parser(buf, rtype)?;
                    let value: &i32 = unsafe { transmute(value_bytes.as_ptr()) };
                    Ok(RegisterValue::I32(*value))
                }
                I16 => {
                    let (_, (_type_id, value_bytes)) = value_parser(buf, rtype)?;
                    let value: &i16 = unsafe { transmute(value_bytes.as_ptr()) };
                    Ok(RegisterValue::I16(*value))
                }
                I8 => {
                    let (_, (_type_id, value_bytes)) = value_parser(buf, rtype)?;
                    let value: &i8 = unsafe { transmute(value_bytes.as_ptr()) };
                    Ok(RegisterValue::I8(*value))
                }
                STR => {
                    let (_, (_type_id, value_bytes)) = value_parser(buf, rtype)?;
                    // Super dangerous here, casting lifetime to 'static
                    let value: &'static str =
                        unsafe { transmute(from_utf8_unchecked(value_bytes)) };
                    Ok(RegisterValue::STR((*value).to_string()))
                }
                BYTES => {
                    let (_, (_type_id, value_bytes)) = value_parser(buf, rtype)?;
                    // Super dangerous here, casting lifetime to 'static
                    let value: &'static [u8] = unsafe { transmute(value_bytes) };
                    Ok(RegisterValue::BYTES((*value).to_vec()))
                }
                UNIT => {
                    Err(Error::BadParam)
                }
            }
        }
        Write => {
            Ok(RegisterValue::UNIT)
        }
        _ => return Err(Error::BadProtocol),
    }
}

#[inline(always)]
fn sign_parser(i: &[u8]) -> ParseResult<()> {
    if i[0] != PROTOCOL_SIGN {
        return Err(Error::BadProtocol);
    }

    Ok((&i[1..], ()))
}

#[inline(always)]
pub fn op_parser(i: &[u8]) -> ParseResult<OperationStatus> {
    use core::convert::TryFrom;

    let val = i[0] as i8;
    match OperationStatus::try_from(val) {
        Ok(v) => Ok((&i[1..], v)),
        Err(_) => Err(Error::BadProtocol),
    }
}

#[inline(always)]
fn name_sz_parser(i: &[u8]) -> ParseResult<usize> {
    let name_sz = i[0] as usize;
    Ok((&i[1..], name_sz))
}


#[inline(always)]
fn name_parser<'a>(i: &'a [u8], sz: usize) -> ParseResult<&'a str> {
    // TODO: think about msg len check and panic behaviour
    use core::str::from_utf8_unchecked;
    let name = unsafe { from_utf8_unchecked(&i[..sz as usize]) };

    Ok((&i[sz..], name))
}

#[inline(always)]
fn value_type(i: &[u8]) -> ParseResult<RegTypeId>  {
    use RegTypeId::*;
    use core::convert::TryFrom;

    if i.len() < 1 { return Err(Error::BadParam) }
    let type_id = i[0];
    //let i = &i[1..];

    let type_id = match RegTypeId::try_from(type_id) {
        Ok(v) => v,
        Err(_) => return Err(Error::BadFormat),
    };
    Ok((&i[1..], type_id))
}

#[inline(always)]
fn value_parser(i: &[u8], type_id: RegTypeId) -> ParseResult<(RegTypeId, &[u8])> {
    use core::convert::TryFrom;
    use RegTypeId::*;

    match type_id {
        UNIT => {
            if i.len() != 0 { return Err(Error::BadParam) }
            Ok((&[], (type_id, &[])))
        }
        I32 | U32 => {
            if i.len() != 4 { return Err(Error::BadParam) }
            Ok((&[], (type_id, i)))
        }
        I16 | U16 => {
            if i.len() != 2 { return Err(Error::BadParam) }
            Ok((&[], (type_id, i)))
        }
        I8 | U8 => {
            if i.len() != 1 { return Err(Error::BadParam) }
            Ok((&[], (type_id, i)))
        }
        STR | BYTES => {
            if i.len() < 1 { return Err(Error::BadParam) }
            let sz = i[0] as usize;
            let i = &i[1..sz+1];

            if i.len() < sz { 
                return Err(Error::BadParam) }
            Ok((&[], (type_id, i)))
        }
    }
}
