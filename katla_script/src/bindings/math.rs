use std::fmt;

use katla_math::{Color, Quat, Transform, Vec3};
use mlua::{FromLua, IntoLua, Lua, MetaMethod, UserData, UserDataFields, UserDataMethods, Value};

macro_rules! impl_from_lua_userdata {
    ($wrapper:ident, $inner:ty) => {
        impl FromLua for $wrapper {
            fn from_lua(value: Value, _lua: &Lua) -> mlua::Result<Self> {
                match value {
                    Value::UserData(ud) => {
                        let borrowed = ud.borrow::<$wrapper>()?;
                        Ok(borrowed.clone())
                    }
                    _ => Err(mlua::Error::FromLuaConversionError {
                        from: value.type_name(),
                        to: stringify!($wrapper).into(),
                        message: Some(format!("expected {} userdata", stringify!($inner)).into()),
                    }),
                }
            }
        }
    };
}

#[derive(Clone, Copy, Debug)]
pub struct LuaVec3(pub Vec3);

impl_from_lua_userdata!(LuaVec3, Vec3);

impl fmt::Display for LuaVec3 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Vec3({}, {}, {})", self.0.x(), self.0.y(), self.0.z())
    }
}

impl UserData for LuaVec3 {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("x", |_, this| Ok(this.0.x()));
        fields.add_field_method_set("x", |_, this, val: f32| {
            this.0.0[0] = val;
            Ok(())
        });
        fields.add_field_method_get("y", |_, this| Ok(this.0.y()));
        fields.add_field_method_set("y", |_, this, val: f32| {
            this.0.0[1] = val;
            Ok(())
        });
        fields.add_field_method_get("z", |_, this| Ok(this.0.z()));
        fields.add_field_method_set("z", |_, this, val: f32| {
            this.0.0[2] = val;
            Ok(())
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_function("new", |_, (x, y, z): (f32, f32, f32)| {
            Ok(LuaVec3(Vec3::new(x, y, z)))
        });

        methods.add_method("length", |_, this, ()| Ok(this.0.length()));
        methods.add_method("length_squared", |_, this, ()| Ok(this.0.length_squared()));
        methods.add_method("normalize", |_, this, ()| Ok(LuaVec3(this.0.normalize())));
        methods.add_method("dot", |_, this, other: LuaVec3| Ok(this.0.dot(other.0)));
        methods.add_method("cross", |_, this, other: LuaVec3| {
            Ok(LuaVec3(this.0.cross(other.0)))
        });
        methods.add_method("lerp", |_, this, (other, t): (LuaVec3, f32)| {
            Ok(LuaVec3(this.0.lerp(other.0, t)))
        });
        methods.add_method("distance", |_, this, other: LuaVec3| {
            Ok(this.0.distance(other.0))
        });

        methods.add_meta_method(MetaMethod::Add, |_, this, other: LuaVec3| {
            Ok(LuaVec3(this.0 + other.0))
        });
        methods.add_meta_method(MetaMethod::Sub, |_, this, other: LuaVec3| {
            Ok(LuaVec3(this.0 - other.0))
        });
        methods.add_meta_function(
            MetaMethod::Mul,
            |_, (lhs, rhs): (LuaVec3, Value)| match rhs {
                Value::Number(n) => Ok(LuaVec3(lhs.0 * n as f32)),
                Value::UserData(ud) => {
                    let other = ud.borrow::<LuaVec3>()?;
                    Ok(LuaVec3(lhs.0 * other.0))
                }
                _ => Err(mlua::Error::runtime(format!(
                    "Vec3.__mul: expected Vec3 or number, got {}",
                    rhs.type_name()
                ))),
            },
        );
        methods.add_meta_method(MetaMethod::Unm, |_, this, ()| Ok(LuaVec3(-this.0)));
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| Ok(this.to_string()));
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LuaQuat(pub Quat);

impl_from_lua_userdata!(LuaQuat, Quat);

impl fmt::Display for LuaQuat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (x, y, z, w) = self.0.xyzw();
        write!(f, "Quat({}, {}, {}, {})", x, y, z, w)
    }
}

impl UserData for LuaQuat {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("x", |_, this| Ok(this.0[0]));
        fields.add_field_method_get("y", |_, this| Ok(this.0[1]));
        fields.add_field_method_get("z", |_, this| Ok(this.0[2]));
        fields.add_field_method_get("w", |_, this| Ok(this.0[3]));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_function("identity", |_, ()| Ok(LuaQuat(Quat::identity())));
        methods.add_function("new", |_, (x, y, z, w): (f32, f32, f32, f32)| {
            Ok(LuaQuat(Quat::new(x, y, z, w)))
        });
        methods.add_function("from_axis_angle", |_, (axis, angle): (LuaVec3, f32)| {
            Ok(LuaQuat(Quat::from_axis_angle(axis.0, angle)))
        });
        methods.add_method("conjugate", |_, this, ()| Ok(LuaQuat(this.0.conjugate())));
        methods.add_method("normalize", |_, this, ()| {
            let mut q = this.0;
            q.normalize();
            Ok(LuaQuat(q))
        });
        methods.add_method("slerp", |_, this, (other, t): (LuaQuat, f32)| {
            Ok(LuaQuat(Quat::slerp(this.0, other.0, t)))
        });

        methods.add_meta_function(MetaMethod::Mul, |lua, (lhs, rhs): (LuaQuat, Value)| {
            let type_name = rhs.type_name();
            match &rhs {
                Value::UserData(ud) => {
                    if let Ok(q) = ud.borrow::<LuaQuat>() {
                        let result = LuaQuat(lhs.0 * q.0);
                        drop(q);
                        result.into_lua(lua)
                    } else if let Ok(v) = ud.borrow::<LuaVec3>() {
                        let result = LuaVec3(lhs.0 * v.0);
                        drop(v);
                        result.into_lua(lua)
                    } else {
                        Err(mlua::Error::runtime(format!(
                            "Quat.__mul: expected Quat or Vec3, got {}",
                            type_name
                        )))
                    }
                }
                _ => Err(mlua::Error::runtime(format!(
                    "Quat.__mul: expected Quat or Vec3, got {}",
                    type_name
                ))),
            }
        });
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| Ok(this.to_string()));
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LuaTransform(pub Transform);

impl_from_lua_userdata!(LuaTransform, Transform);

impl UserData for LuaTransform {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("position", |_, this| Ok(LuaVec3(this.0.position)));
        fields.add_field_method_set("position", |_, this, val: LuaVec3| {
            this.0.position = val.0;
            Ok(())
        });
        fields.add_field_method_get("scale", |_, this| Ok(LuaVec3(this.0.scale)));
        fields.add_field_method_set("scale", |_, this, val: LuaVec3| {
            this.0.scale = val.0;
            Ok(())
        });
        fields.add_field_method_get("rotation", |_, this| Ok(LuaQuat(this.0.rotation)));
        fields.add_field_method_set("rotation", |_, this, val: LuaQuat| {
            this.0.rotation = val.0;
            Ok(())
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("forward", |_, this, ()| Ok(LuaVec3(this.0.forward())));
        methods.add_method("up", |_, this, ()| Ok(LuaVec3(this.0.up())));
        methods.add_method("right", |_, this, ()| Ok(LuaVec3(this.0.right())));
        methods.add_method("look_at", |_, this, target: LuaVec3| {
            Ok(LuaTransform(this.0.look_at(target.0, Vec3::UP)))
        });
        methods.add_method("lerp", |_, this, (other, t): (LuaTransform, f32)| {
            Ok(LuaTransform(this.0.lerp(&other.0, t)))
        });
        methods.add_method("inverse", |_, this, ()| Ok(LuaTransform(this.0.inverse())));
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LuaColor(pub Color);

impl_from_lua_userdata!(LuaColor, Color);

impl fmt::Display for LuaColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Color({}, {}, {}, {})",
            self.0.r, self.0.g, self.0.b, self.0.a
        )
    }
}

impl UserData for LuaColor {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("r", |_, this| Ok(this.0.r));
        fields.add_field_method_set("r", |_, this, val: f32| {
            this.0.r = val;
            Ok(())
        });
        fields.add_field_method_get("g", |_, this| Ok(this.0.g));
        fields.add_field_method_set("g", |_, this, val: f32| {
            this.0.g = val;
            Ok(())
        });
        fields.add_field_method_get("b", |_, this| Ok(this.0.b));
        fields.add_field_method_set("b", |_, this, val: f32| {
            this.0.b = val;
            Ok(())
        });
        fields.add_field_method_get("a", |_, this| Ok(this.0.a));
        fields.add_field_method_set("a", |_, this, val: f32| {
            this.0.a = val;
            Ok(())
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_function("new", |_, (r, g, b, a): (f32, f32, f32, f32)| {
            Ok(LuaColor(Color::new(r, g, b, a)))
        });
        methods.add_function("rgb", |_, (r, g, b): (f32, f32, f32)| {
            Ok(LuaColor(Color::rgb(r, g, b)))
        });
        methods.add_function("from_rgb_hex", |_, hex: u32| {
            Ok(LuaColor(Color::from_rgb_hex(hex)))
        });
        methods.add_method("with_alpha", |_, this, alpha: f32| {
            Ok(LuaColor(this.0.with_alpha(alpha)))
        });
        methods.add_method("lerp", |_, this, (other, t): (LuaColor, f32)| {
            Ok(LuaColor(Color::lerp(this.0, other.0, t)))
        });
        methods.add_method("clamped", |_, this, ()| Ok(LuaColor(this.0.clamped())));

        methods.add_meta_method(MetaMethod::Add, |_, this, other: LuaColor| {
            Ok(LuaColor(this.0 + other.0))
        });
        methods.add_meta_method(MetaMethod::Sub, |_, this, other: LuaColor| {
            Ok(LuaColor(this.0 - other.0))
        });
        methods.add_meta_function(
            MetaMethod::Mul,
            |_, (lhs, rhs): (LuaColor, Value)| match rhs {
                Value::Number(n) => Ok(LuaColor(lhs.0 * n as f32)),
                _ => Err(mlua::Error::runtime(format!(
                    "Color.__mul: expected number, got {}",
                    rhs.type_name()
                ))),
            },
        );
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| Ok(this.to_string()));
    }
}

pub fn register_math_types(lua: &Lua) -> Result<(), mlua::Error> {
    lua.register_userdata_type::<LuaVec3>(|reg| {
        LuaVec3::add_fields(reg);
        LuaVec3::add_methods(reg);
    })?;
    lua.register_userdata_type::<LuaQuat>(|reg| {
        LuaQuat::add_fields(reg);
        LuaQuat::add_methods(reg);
    })?;
    lua.register_userdata_type::<LuaTransform>(|reg| {
        LuaTransform::add_fields(reg);
        LuaTransform::add_methods(reg);
    })?;
    lua.register_userdata_type::<LuaColor>(|reg| {
        LuaColor::add_fields(reg);
        LuaColor::add_methods(reg);
    })?;
    Ok(())
}
