package io.bazeltools.buildgen.scaladefref

import org.scalatest.funsuite.AnyFunSuite
import io.bazeltools.buildgen.shared._
import scala.collection.immutable.SortedSet
import cats.effect.unsafe.implicits.global

class CanParseFileTest extends AnyFunSuite {

  def assertParse(str: String, expected: DataBlock) =
    assert( ScalaSourceEntityExtractor.extract(str).unsafeRunSync() === expected)

//   test("can extract a simple file") {
//     val simpleContent = """
//     package com.foo.bar

//     case class Cat(foo: String)
//     """
//     val expectedDataBlock = DataBlock(
//          "",
//         defs = SortedSet(Entity.dotted("com.foo.bar.Cat"), Entity.dotted("com.foo.bar.Cat.foo")),
//         refs = SortedSet(Entity.dotted("String"), Entity.dotted("com.foo.bar.String")),
//         bzl_gen_build_commands = SortedSet()
//     )
//     assertParse(simpleContent, expectedDataBlock)
//   }

//     test("can extract a strange syntax") {
//     val simpleContent = """
//     package com.foo.bar

//     case class Cat(foo: String) {
//         val expressionName = Dog()
//         // This format means we will throw a match error if Lizard != Dog with match
//         val (`expressionName`, pie) = (Lizard(), 33)
//     }
//     """



//     val expectedDataBlock = DataBlock("", SortedSet(
//             Entity.dotted("com.foo.bar.Cat"),
//             Entity.dotted("com.foo.bar.Cat.expressionName"),
//             Entity.dotted("com.foo.bar.Cat.foo"),
//             Entity.dotted("com.foo.bar.Cat.pie")
//             ),
//             SortedSet(
//                 Entity.dotted("Dog"),
//                 Entity.dotted("Lizard"),
//                 Entity.dotted("String"),
//                 Entity.dotted("com.foo.bar.Dog"),
//                 Entity.dotted("com.foo.bar.Lizard"),
//                 Entity.dotted("com.foo.bar.String")
//                 ), SortedSet())
//     assertParse(simpleContent, expectedDataBlock)
//   }

//   test("can extract a failing file") {
//     val simpleContent = """
//     package com.foo.bar

//     object TestObj {
//         def test(typ: TensorDataType) = forAll(genSparseTensorOf(typ)) {
//         tensor =>
//           val elementType = CustomObject.ElementType(typ)
//           val expectedSparkType = CustomObject.SparseType(elementType)
//           val (`expectedSparkType`, metadata, writer) = CustomObject.writer(tensor.getFlavor, tensor.getDataType, tensor.getShape)
//           val reader = CustomObject.sparseReader(elementType, tensor.getShape)
//           val read = reader(writer(tensor).asInstanceOf[InternalRow])

//           if (read != tensor) {
//             println("Tensors don't match")
//           }

//           read shouldEqual tensor
//       }
//     }
//     """
//     val expectedDataBlock = DataBlock("",
// SortedSet(
//     Entity.dotted("com.foo.bar.TestObj"),
//     Entity.dotted("com.foo.bar.TestObj.test")
// ),
// SortedSet(
// Entity.dotted("CustomObject"),
// Entity.dotted("CustomObject.ElementType"),
// Entity.dotted("CustomObject.SparseType"),
// Entity.dotted("CustomObject.sparseReader"),
// Entity.dotted("CustomObject.writer"),
// Entity.dotted("InternalRow"),
// Entity.dotted("TensorDataType"),
// Entity.dotted("com.foo.bar.CustomObject"),
// Entity.dotted("com.foo.bar.CustomObject.ElementType"),
// Entity.dotted("com.foo.bar.CustomObject.SparseType"),
// Entity.dotted("com.foo.bar.CustomObject.sparseReader"),
// Entity.dotted("com.foo.bar.CustomObject.writer"),
// Entity.dotted("com.foo.bar.InternalRow"),
// Entity.dotted("com.foo.bar.TensorDataType"),
// Entity.dotted("com.foo.bar.forAll"),
// Entity.dotted("com.foo.bar.genSparseTensorOf"),
// Entity.dotted("com.foo.bar.println"),
// Entity.dotted("forAll"),
// Entity.dotted("genSparseTensorOf"),
// Entity.dotted("println"),
// ),
// SortedSet()
//     )
//     assertParse(simpleContent, expectedDataBlock)
//   }

//   test("can extract a failing file 2") {
//     val simpleContent = """
// package com.foo.bar
// trait TestTrait {
//   def foo(): Long = {
//       combinedParams.underlying.collect {
//         case (param: OtherTrait[param], value: String) if outCols.contains(param) => (param, value)
//       }
//   }
// }
//     """
//     val expectedDataBlock = DataBlock("",
// SortedSet(
//     Entity.dotted("com.foo.bar.TestTrait"),
//     Entity.dotted("com.foo.bar.TestTrait.foo"),
// ),
// SortedSet(Entity.dotted("Long"),
//     Entity.dotted("OtherTrait"),
//     Entity.dotted("String"),
//     Entity.dotted("com.foo.bar.Long"),
//     Entity.dotted("com.foo.bar.OtherTrait"),
//     Entity.dotted("com.foo.bar.String"),
//     Entity.dotted("com.foo.bar.combinedParams"),
//     Entity.dotted("com.foo.bar.combinedParams.underlying"),
//     Entity.dotted("com.foo.bar.combinedParams.underlying.collect"),
//     Entity.dotted("com.foo.bar.outCols"),
//     Entity.dotted("com.foo.bar.outCols.contains"),
//     Entity.dotted("com.foo.bar.param"),
//     Entity.dotted("com.foo.bar.value"),
//     Entity.dotted("combinedParams"),
//     Entity.dotted("combinedParams.underlying"),
//     Entity.dotted("combinedParams.underlying.collect"),
//     Entity.dotted("outCols"),
//     Entity.dotted("outCols.contains"),
//     Entity.dotted("param"),
//     Entity.dotted("value"),
// ),
// SortedSet())

//     assertParse(simpleContent, expectedDataBlock)
//   }



    test("Add transitive links") {
    val simpleContent = """
        package com.foo.bar

        case class Cat(foo: String) extends Dog
    """
    val expectedDataBlock = DataBlock(
         "",
        defs = SortedSet(Entity.dotted("com.foo.bar.Cat"), Entity.dotted("com.foo.bar.Cat.foo")),
        refs = SortedSet(Entity.dotted("Dog"), Entity.dotted("String"),Entity.dotted("com.foo.bar.Dog"), Entity.dotted("com.foo.bar.String")),
        bzl_gen_build_commands = SortedSet("link: com.foo.bar.Cat -> Dog", "link: com.foo.bar.Cat -> com.foo.bar.Dog")
    )
    assertParse(simpleContent, expectedDataBlock)
  }

      test("Add more transitive links") {
    val simpleContent = """
        package com.foo.bar

  sealed abstract class PromotionEventColParam
    extends AbstractStructColParam[PromotionEventDataStruct, PromotionEvent, PromotionEventColParam]{}
    """
    val expectedDataBlock = DataBlock(
         "",
        defs = SortedSet(Entity.dotted("com.foo.bar.Cat"), Entity.dotted("com.foo.bar.Cat.foo")),
        refs = SortedSet(Entity.dotted("Dog"), Entity.dotted("String"),Entity.dotted("com.foo.bar.Dog"), Entity.dotted("com.foo.bar.String")),
        bzl_gen_build_commands = SortedSet("link: com.foo.bar.Cat -> Dog", "link: com.foo.bar.Cat -> com.foo.bar.Dog")
    )
    assertParse(simpleContent, expectedDataBlock)
  }





}
