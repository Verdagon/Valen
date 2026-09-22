use crate::interner::StrI;
use crate::utils::range::RangeS;
use crate::utils::arena_index_map::ArenaIndexMap;
use crate::postparsing::names::IRuneS;
use crate::instantiating::ast::types::{KindIT, ICitizenIT, SharednessI, StructIT};
use crate::instantiating::ast::names::{
    IdI, INameI,
    IFunctionNameI, IImplNameI, IInterfaceNameI, IStructNameI, ICitizenNameI,
    IRegionNameI, IVarNameI,
    ExportNameI, FunctionBoundNameI, ImplBoundNameI,
};
use crate::instantiating::ast::expressions::ExpressionIE;
use crate::instantiating::ast::types::InterfaceIT;
use crate::utils::code_hierarchy::PackageCoordinate;
use std::hash::Hash;
use std::hash::Hasher;
use std::ptr::eq;
use std::ptr::hash;




#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct KindExportI<'s, 'i> {
    pub range: RangeS<'s>,
    pub tyype: KindIT<'s, 'i>,
    pub id: IdI<'s, 'i>,
    pub exported_name: StrI<'s>,
}






#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct FunctionExportI<'s, 'i> where 's: 'i {
    pub range: RangeS<'s>,
    pub prototype: &'i PrototypeI<'s, 'i>,
    pub export_id: IdI<'s, 'i>,
    pub exported_name: StrI<'s>,
}





#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct FunctionExternI<'s, 'i> where 's: 'i {
    pub prototype: &'i PrototypeI<'s, 'i>,
    pub num_inherited_generic_parameters: i32,
    pub link_name: &'i str,
}






#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct KindExternI<'s, 'i> where 's: 'i {
    pub r#struct: &'i StructIT<'s, 'i>,
}







#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct InterfaceEdgeBlueprintI<'s, 'i> where 's: 'i {
    pub interface: IdI<'s, 'i>,
    pub super_family_root_headers: &'i [(&'i PrototypeI<'s, 'i>, i32)],
}





#[derive(PartialEq, Eq, Debug)]
pub struct EdgeI<'s, 'i> where 's: 'i {
    pub edge_id: IdI<'s, 'i>,
    pub sub_citizen: ICitizenIT<'s, 'i>,
    pub super_interface: IdI<'s, 'i>,
    pub rune_to_func_bound: ArenaIndexMap<'i, IRuneS<'s>, IdI<'s, 'i>>,
    pub rune_to_impl_bound: ArenaIndexMap<'i, IRuneS<'s>, IdI<'s, 'i>>,
    pub abstract_func_to_override_func: ArenaIndexMap<'i, IdI<'s, 'i>, &'i PrototypeI<'s, 'i>>,
}





#[derive(Debug)]
pub struct FunctionDefinitionI<'s, 'i> where 's: 'i {
    pub header: FunctionHeaderI<'s, 'i>,
    pub rune_to_func_bound: ArenaIndexMap<'i, IRuneS<'s>, IdI<'s, 'i>>,
    pub rune_to_impl_bound: ArenaIndexMap<'i, IRuneS<'s>, IdI<'s, 'i>>,
    pub body: ExpressionIE<'s, 'i>,
}


#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct LocI<'i> {
    pub path: &'i [i32],
}



impl<'i> LocI<'i> {
    pub fn add(&self, sub_location: i32) -> LocI<'i> {
        panic!("Unimplemented: add")
        // LocationInFunctionEnvironmentI(path :+ subLocation)
    }


    pub fn to_string(&self) -> String {
        self.path.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(".")
    }
}



#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct AbstractI;




#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct ParameterI<'s, 'i> where 's: 'i {
    pub name: IVarNameI<'s, 'i>,
    pub virtuality: Option<AbstractI>,
    pub tyype: KindIT<'s, 'i>,
}




impl<'s, 'i> ParameterI<'s, 'i> {
    pub fn same(&self, that: &ParameterI<'_, '_>) -> bool {
        panic!("Unimplemented: same")
        // name == that.name && virtuality == that.virtuality && tyype == that.tyype
    }
}



#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct SignatureI<'s, 'i> {
    pub id: IdI<'s, 'i>,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum IFunctionAttributeI<'s> {
    PureI,
    UserFunctionI,
    ExternI(ExternI<'s>),
}




#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ICitizenAttributeI<'s> {
    SealedI,
    ExternI(ExternI<'s>),
}



#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct ExternI<'s> {
    pub package_coord: PackageCoordinate<'s>,
}




#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct RegionI<'s, 'i> where 's: 'i {
    pub name: IRegionNameI<'s, 'i>,
    pub mutable: bool,
}




#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct FunctionHeaderI<'s, 'i> where 's: 'i {
    // This one little name field can illuminate much of how the compiler works, see UINIT.
    pub id: IdI<'s, 'i>,
    pub attributes: &'i [IFunctionAttributeI<'s>],
//  regions: Vector[cIegionI],
    pub params: &'i [ParameterI<'s, 'i>],
    pub return_type: KindIT<'s, 'i>,
}




impl<'s, 'i> FunctionHeaderI<'s, 'i> {
    pub fn is_extern(&self) -> bool {
        panic!("Unimplemented: is_extern")
        // attributes.exists({ case ExternI(_) => true case _ => false })
    }


    pub fn is_user_function(&self) -> bool {
        self.attributes.contains(&IFunctionAttributeI::UserFunctionI)
    }


    pub fn get_abstract_interface(&self) -> Option<&'i InterfaceIT<'s, 'i>> {
        let abstract_interfaces: Vec<_> = self.params.iter().filter_map(|p| match (p.virtuality, p.tyype) {
            (Some(AbstractI), KindIT::InterfaceIT(ir)) => Some(ir),
            _ => None,
        }).collect();
        assert!(abstract_interfaces.len() <= 1);
        abstract_interfaces.into_iter().next()
    }


    pub fn get_virtual_index(&self) -> Option<i32> {
        panic!("Unimplemented: get_virtual_index")
        // val indices = params.zipWithIndex.collect({ case (ParameterI(_, Some(AbstractI()), _, _), index) => index })
        // vassert(indices.size <= 1)
        // indices.headOption
    }


    pub fn to_prototype(&self) -> PrototypeI<'s, 'i> {
        PrototypeI { id: self.id, return_type: self.return_type }
    }


    pub fn to_signature(&self) -> SignatureI<'_, '_> {
        panic!("Unimplemented: to_signature")
        // toPrototype.toSignature
    }
}


impl<'s, 'i> FunctionHeaderI<'s, 'i> where 's: 'i {
    pub fn param_types(&self) -> Vec<KindIT<'s, 'i>> {
        IFunctionNameI::try_from(self.id.local_name).unwrap().parameters().to_vec()
    }
}



impl<'s, 'i> FunctionHeaderI<'s, 'i> {
    pub fn is_pure(&self) -> bool {
        panic!("Unimplemented: is_pure")
        // attributes.collectFirst({ case PureI => }).nonEmpty
    }
}



#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct PrototypeI<'s, 'i> {
    pub id: IdI<'s, 'i>,
    pub return_type: KindIT<'s, 'i>,
}



impl<'s, 'i> PrototypeI<'s, 'i> where 's: 'i {
    pub fn param_types(&self) -> Vec<KindIT<'s, 'i>> {
        IFunctionNameI::try_from(self.id.local_name).unwrap().parameters().to_vec()
    }
}


impl<'s, 'i> PrototypeI<'s, 'i> {
    pub fn to_signature(&self) -> SignatureI<'s, 'i> {
        SignatureI { id: self.id }
    }
}


#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum IVariableI<'s, 'i> where 's: 'i {
    Local(&'i LocalVariableI<'s, 'i>),
    Capture(&'i CapturedVariableI<'s, 'i>),
}



impl<'s, 'i> IVariableI<'s, 'i> where 's: 'i {
    pub fn name(&self) -> IVarNameI<'s, 'i> {
        match self {
            IVariableI::Local(v) => v.name,
            IVariableI::Capture(v) => v.name,
        }
    }
}


#[derive(Debug)]
pub struct LocalVariableI<'s, 'i> where 's: 'i {
    pub name: IVarNameI<'s, 'i>,
    pub tyype: KindIT<'s, 'i>,
}

impl<'s, 'i> PartialEq for LocalVariableI<'s, 'i> where 's: 'i {
    fn eq(&self, other: &Self) -> bool {
        eq(self, other)
    }
}
impl<'s, 'i> Eq for LocalVariableI<'s, 'i> where 's: 'i {}
impl<'s, 'i> Hash for LocalVariableI<'s, 'i> where 's: 'i {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash(self, state)
    }
}


#[derive(Debug)]
pub struct CapturedVariableI<'s, 'i> where 's: 'i {
    pub name: IVarNameI<'s, 'i>,
    pub closured_vars_struct_type: &'i StructIT<'s, 'i>,
    pub tyype: KindIT<'s, 'i>,
}

impl<'s, 'i> PartialEq for CapturedVariableI<'s, 'i> where 's: 'i {
    fn eq(&self, other: &Self) -> bool {
        eq(self, other)
    }
}
impl<'s, 'i> Eq for CapturedVariableI<'s, 'i> where 's: 'i {}
impl<'s, 'i> Hash for CapturedVariableI<'s, 'i> where 's: 'i {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash(self, state)
    }
}

impl<'s, 'i> From<&'i LocalVariableI<'s, 'i>> for IVariableI<'s, 'i> {
    fn from(v: &'i LocalVariableI<'s, 'i>) -> Self {
        IVariableI::Local(v)
    }
}


